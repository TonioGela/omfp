//! What request handlers use: the store-backed services, the agent supervisor, and the logs of the `index` and
//! `workspace/<id>` topics. Workspace folders are rescanned here (DMN-023), and a change reaches the index and the
//! roots of the live threads in the workspace (DMN-029).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use omfp_protocol::{
    ErrorBody, IndexEvent, IndexSnapshot, RepoStatus, ThreadSummary, Workspace, WorkspaceEvent,
};
use tokio::task::JoinHandle;
use tracing::warn;

use super::convert::{self, internal, workspace_error};
use crate::event_log::EventLog;
use crate::store::{Store, StoreError, new_id, now_ms};
use crate::supervisor::Supervisor;
use crate::threads::Threads;
use crate::workspaces::{WorkspaceView, Workspaces};

/// The `index` topic: repositories, workspaces and thread summaries.
pub(crate) type IndexLog = EventLog<IndexEvent, IndexSnapshot>;

/// A `workspace/<id>` topic: the git status of each repository of the workspace.
pub(crate) type WorkspaceLog = EventLog<WorkspaceEvent, Vec<RepoStatus>>;

const INDEX_CAPACITY: usize = 512;
const WORKSPACE_CAPACITY: usize = 16;
const LIVE_BUFFER: usize = 1024;

#[allow(clippy::trivially_copy_pass_by_ref)] // `EventLog` takes `fn(&mut S, &E)`.
fn apply_index(state: &mut IndexSnapshot, event: &IndexEvent) {
    fn upsert<T: Clone>(items: &mut Vec<T>, item: &T, same: impl Fn(&T) -> bool) {
        match items.iter_mut().find(|existing| same(existing)) {
            Some(existing) => existing.clone_from(item),
            None => items.push(item.clone()),
        }
    }
    match event {
        IndexEvent::RepositoryUpserted { repository } => {
            upsert(&mut state.repositories, repository, |r| {
                r.id == repository.id
            });
        }
        IndexEvent::RepositoryRemoved { repository_id } => {
            state.repositories.retain(|r| r.id != *repository_id);
        }
        IndexEvent::WorkspaceUpserted { workspace } => {
            upsert(&mut state.workspaces, workspace, |w| w.id == workspace.id);
        }
        IndexEvent::WorkspaceRemoved { workspace_id } => {
            state.workspaces.retain(|w| w.id != *workspace_id);
        }
        IndexEvent::ThreadUpserted { thread } => {
            upsert(&mut state.threads, thread, |t| t.id == thread.id);
        }
        IndexEvent::ThreadRemoved { thread_id } => {
            state.threads.retain(|t| t.id != *thread_id);
        }
    }
}

#[allow(clippy::trivially_copy_pass_by_ref)] // `EventLog` takes `fn(&mut S, &E)`.
fn apply_workspace(state: &mut Vec<RepoStatus>, event: &WorkspaceEvent) {
    match event {
        WorkspaceEvent::Status { repos } => state.clone_from(repos),
    }
}

/// Shared by every connection. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Services {
    pub store: Arc<Store>,
    pub threads: Arc<Threads>,
    pub workspaces: Arc<Workspaces>,
    pub supervisor: Supervisor,
    pub index: Arc<IndexLog>,
    /// Serializes index refreshes so their events never interleave.
    refresh: Arc<tokio::sync::Mutex<()>>,
    workspace_logs: Arc<Mutex<HashMap<String, Arc<WorkspaceLog>>>>,
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ErrorBody> + Send + 'static,
) -> Result<T, ErrorBody> {
    tokio::task::spawn_blocking(work).await.map_err(internal)?
}

impl Services {
    /// The services over the daemon's stores; every workspace is scanned first (DMN-023).
    pub async fn new(
        store: Arc<Store>,
        threads: Arc<Threads>,
        workspaces: Arc<Workspaces>,
        supervisor: Supervisor,
    ) -> Result<Self, ErrorBody> {
        let services = Self {
            store,
            threads,
            workspaces,
            supervisor,
            index: Arc::new(EventLog::new(
                new_id(),
                IndexSnapshot {
                    repositories: Vec::new(),
                    workspaces: Vec::new(),
                    threads: Vec::new(),
                },
                apply_index,
                INDEX_CAPACITY,
                LIVE_BUFFER,
            )),
            refresh: Arc::new(tokio::sync::Mutex::new(())),
            workspace_logs: Arc::new(Mutex::new(HashMap::new())),
        };
        let workspaces = Arc::clone(&services.workspaces);
        blocking(move || {
            for ws in workspaces.list().map_err(workspace_error)? {
                if let Err(error) = workspaces.rescan(&ws.id) {
                    warn!(workspace = %ws.id, %error, "cannot scan the workspace folder");
                }
            }
            Ok(())
        })
        .await?;
        services.refresh_index().await?;
        Ok(services)
    }

    /// Rescans the workspace of every thread that finishes a tool execution (DMN-023).
    pub fn spawn_rescans(&self) -> JoinHandle<()> {
        let mut tool_ends = self.supervisor.subscribe_tool_ends();
        let services = self.clone();
        tokio::spawn(async move {
            loop {
                let thread_id = match tool_ends.recv().await {
                    Ok(thread_id) => thread_id,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                };
                let threads = Arc::clone(&services.threads);
                let workspace = blocking(move || {
                    threads
                        .get(&thread_id)
                        .map(|thread| thread.workspace_id)
                        .map_err(convert::thread_error)
                })
                .await;
                let result = match workspace {
                    Ok(workspace_id) => services.rescan(&workspace_id).await.map(drop),
                    Err(error) => Err(error),
                };
                if let Err(error) = result {
                    warn!(error = %error.message, "rescan after a tool run failed");
                }
            }
        })
    }

    /// Rescans a workspace folder (DMN-023): a change updates the `index` topic, and the live threads in the
    /// workspace get their roots synced (DMN-029).
    pub async fn rescan(&self, id: &str) -> Result<WorkspaceView, ErrorBody> {
        let workspaces = Arc::clone(&self.workspaces);
        let workspace_id = id.to_owned();
        let (view, changed) =
            blocking(move || workspaces.rescan(&workspace_id).map_err(workspace_error)).await?;
        if changed {
            self.refresh_index_quietly().await;
        }
        self.sync_roots(id).await?;
        Ok(view)
    }

    /// Syncs the roots of the live threads living in workspace `id` with its last scan (DMN-029).
    pub async fn sync_roots(&self, id: &str) -> Result<(), ErrorBody> {
        let store = Arc::clone(&self.store);
        let workspace_id = id.to_owned();
        let thread_ids = blocking(move || {
            store
                .workspace_thread_ids(&workspace_id)
                .map_err(ErrorBody::from)
        })
        .await?;
        for thread_id in thread_ids {
            if self.supervisor.is_running(&thread_id)
                && let Err(error) = self.supervisor.sync_roots(&thread_id).await
            {
                warn!(thread = %thread_id, %error, "root sync failed");
            }
        }
        Ok(())
    }

    /// Keeps thread summaries current when processes start, stop, settle or ask something.
    pub fn spawn_index_updates(&self) -> JoinHandle<()> {
        let mut changes = self.supervisor.subscribe_changes();
        let services = self.clone();
        tokio::spawn(async move {
            while let Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) =
                changes.recv().await
            {
                services.refresh_index_quietly().await;
            }
        })
    }

    /// Reloads repositories, workspaces and threads and publishes what changed on the `index` topic.
    pub async fn refresh_index(&self) -> Result<(), ErrorBody> {
        let _serial = self.refresh.lock().await;
        // Only refreshes change the index, and they are serialized: `current` stays current.
        let current = Arc::new(self.index.with_state(Clone::clone));
        let previous = Arc::clone(&current);
        let store = Arc::clone(&self.store);
        let workspaces = Arc::clone(&self.workspaces);
        let supervisor = self.supervisor.clone();
        let fresh =
            blocking(move || load_index(&store, &workspaces, &supervisor, &previous)).await?;
        for event in diff_index(&current, fresh) {
            self.index.push(event);
        }
        Ok(())
    }

    /// Refreshes the index, logging instead of failing: the request that changed something already succeeded.
    pub async fn refresh_index_quietly(&self) {
        if let Err(error) = self.refresh_index().await {
            warn!(error = %error.message, "index refresh failed");
        }
    }

    /// The thread's summary as published on the `index` topic.
    pub async fn thread_summary(&self, id: &str) -> Result<ThreadSummary, ErrorBody> {
        self.refresh_index().await?;
        self.index
            .with_state(|index| index.threads.iter().find(|thread| thread.id == id).cloned())
            .ok_or_else(|| {
                ErrorBody::from(StoreError::NotFound {
                    entity: "thread",
                    id: id.to_owned(),
                })
            })
    }

    /// The workspace as published on the `index` topic.
    pub async fn workspace_summary(&self, id: &str) -> Result<Workspace, ErrorBody> {
        self.refresh_index().await?;
        self.index
            .with_state(|index| index.workspaces.iter().find(|ws| ws.id == id).cloned())
            .ok_or_else(|| {
                ErrorBody::from(StoreError::NotFound {
                    entity: "workspace",
                    id: id.to_owned(),
                })
            })
    }

    /// The log of `workspace/<id>`, its status refreshed.
    pub async fn workspace_log(&self, id: &str) -> Result<Arc<WorkspaceLog>, ErrorBody> {
        self.workspace_status(id).await?;
        Ok(self.workspace_log_entry(id))
    }

    fn workspace_log_entry(&self, id: &str) -> Arc<WorkspaceLog> {
        let mut logs = self
            .workspace_logs
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Arc::clone(logs.entry(id.to_owned()).or_insert_with(|| {
            Arc::new(EventLog::new(
                new_id(),
                Vec::new(),
                apply_workspace,
                WORKSPACE_CAPACITY,
                LIVE_BUFFER,
            ))
        }))
    }

    /// The git status of every repository of the workspace now (DMN-026), published on its topic when it changed.
    pub async fn workspace_status(&self, id: &str) -> Result<Vec<RepoStatus>, ErrorBody> {
        let workspaces = Arc::clone(&self.workspaces);
        let workspace_id = id.to_owned();
        let repos = blocking(move || {
            let repos = workspaces
                .discovered(&workspace_id)
                .map_err(workspace_error)?;
            repos
                .iter()
                .map(|repo| {
                    let (status, upstream) = workspaces.status(repo).map_err(workspace_error)?;
                    let diff = workspaces.diff(repo).map_err(workspace_error)?;
                    Ok(RepoStatus {
                        name: repo.name.clone(),
                        status: convert::git_status(&status, upstream.as_deref(), &diff),
                    })
                })
                .collect::<Result<Vec<_>, ErrorBody>>()
        })
        .await?;
        let log = self.workspace_log_entry(id);
        if log.with_state(|current| *current != repos) {
            log.push(WorkspaceEvent::Status {
                repos: repos.clone(),
            });
        }
        Ok(repos)
    }
}

/// The index as stored and scanned now; thread summaries keep their `updated_at` unless something in them changed.
fn load_index(
    store: &Store,
    workspaces: &Workspaces,
    supervisor: &Supervisor,
    previous: &IndexSnapshot,
) -> Result<IndexSnapshot, ErrorBody> {
    let clients = store.created_clients()?;
    let repositories = store
        .repositories()?
        .iter()
        .map(convert::repository)
        .collect();
    let threads = store.threads()?;
    let mut scanned = Vec::new();
    for ws in workspaces.list().map_err(workspace_error)? {
        let repos = workspaces.discovered(&ws.id).map_err(workspace_error)?;
        scanned.push((ws, repos));
    }
    // Which workspaces hold each checkout (DMN-027).
    let mut holders: HashMap<PathBuf, Vec<String>> = HashMap::new();
    for (ws, repos) in &scanned {
        for repo in repos {
            holders
                .entry(repo.canonical.clone())
                .or_default()
                .push(ws.id.clone());
        }
    }
    let workspaces = scanned
        .iter()
        .map(|(ws, repos)| {
            let thread_ids = threads
                .iter()
                .filter(|thread| thread.workspace_id == ws.id)
                .map(|thread| thread.id.clone())
                .collect();
            convert::workspace(ws, repos, thread_ids, &holders, &clients)
        })
        .collect();
    let threads = threads
        .iter()
        .map(|thread| {
            let before = previous
                .threads
                .iter()
                .find(|summary| summary.id == thread.id);
            let updated_at = before.map_or(thread.created_at, |before| before.updated_at);
            let mut summary = convert::thread_summary(
                thread,
                supervisor.status(&thread.id),
                &clients,
                updated_at,
            );
            if before.is_some_and(|before| *before != summary) {
                summary.updated_at = now_ms();
            }
            summary
        })
        .collect();
    Ok(IndexSnapshot {
        repositories,
        workspaces,
        threads,
    })
}

/// The events turning `current` into `fresh`.
fn diff_index(current: &IndexSnapshot, fresh: IndexSnapshot) -> Vec<IndexEvent> {
    let mut events = Vec::new();
    for repository in &current.repositories {
        if !fresh.repositories.iter().any(|r| r.id == repository.id) {
            events.push(IndexEvent::RepositoryRemoved {
                repository_id: repository.id.clone(),
            });
        }
    }
    for repository in fresh.repositories {
        if !current.repositories.contains(&repository) {
            events.push(IndexEvent::RepositoryUpserted { repository });
        }
    }
    for workspace in &current.workspaces {
        if !fresh.workspaces.iter().any(|w| w.id == workspace.id) {
            events.push(IndexEvent::WorkspaceRemoved {
                workspace_id: workspace.id.clone(),
            });
        }
    }
    for workspace in fresh.workspaces {
        if !current.workspaces.contains(&workspace) {
            events.push(IndexEvent::WorkspaceUpserted { workspace });
        }
    }
    for thread in &current.threads {
        if !fresh.threads.iter().any(|t| t.id == thread.id) {
            events.push(IndexEvent::ThreadRemoved {
                thread_id: thread.id.clone(),
            });
        }
    }
    for thread in fresh.threads {
        if !current.threads.contains(&thread) {
            events.push(IndexEvent::ThreadUpserted { thread });
        }
    }
    events
}
