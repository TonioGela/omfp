//! Thread records (02-daemon, "Threads and workspaces"): creation with a home and a session directory, workspace
//! attachment, metadata. Process supervision lives elsewhere; this module never touches omp.
//!
//! Calls are blocking; the server layer runs them on blocking tasks.

use std::path::PathBuf;
use std::sync::Arc;

use crate::paths::DataDir;
use crate::store::{Provenance, Store, StoreError, Thread, new_id, now_ms};

#[derive(Debug, thiserror::Error)]
pub enum ThreadError {
    #[error("workspace {0} is listed twice")]
    DuplicateWorkspace(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = ThreadError> = std::result::Result<T, E>;

/// Parameters of [`Threads::create`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewThread {
    pub title: String,
    /// Ordered, possibly empty (ARC-008).
    pub workspace_ids: Vec<String>,
    pub tags: Vec<String>,
    /// `provider/id`.
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub approval_mode: Option<String>,
    pub created_by: Provenance,
}

impl NewThread {
    /// A thread created by the user, with no workspace, tag or setting.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            workspace_ids: Vec::new(),
            tags: Vec::new(),
            model: None,
            thinking_level: None,
            approval_mode: None,
            created_by: Provenance::User,
        }
    }
}

/// A root to add to or remove from a live omp process (`/add-dir`, `/remove-dir`, DMN-029).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirChange {
    Added(PathBuf),
    Removed(PathBuf),
}

/// Result of [`Threads::attach`] / [`Threads::detach`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub thread: Thread,
    /// `None` when the set of attached directories did not change (only the order, or nothing).
    pub change: Option<DirChange>,
}

#[derive(Debug)]
pub struct Threads {
    store: Arc<Store>,
    data: DataDir,
}

impl Threads {
    #[must_use]
    pub fn new(store: Arc<Store>, data: DataDir) -> Self {
        Self { store, data }
    }

    /// Creates a thread with its home (omp cwd, DMN-028) and session directory (ARC-020).
    pub fn create(&self, new: NewThread) -> Result<Thread> {
        self.store.check_provenance(&new.created_by)?;
        for (index, id) in new.workspace_ids.iter().enumerate() {
            self.live_workspace_path(id)?;
            if new.workspace_ids[..index].contains(id) {
                return Err(ThreadError::DuplicateWorkspace(id.clone()));
            }
        }
        let id = new_id();
        let session_dir = std::path::absolute(self.data.thread_sessions(&id))?;
        std::fs::create_dir_all(self.data.thread_home(&id))?;
        std::fs::create_dir_all(&session_dir)?;
        let thread = Thread {
            id,
            title: new.title,
            session_dir,
            omp_session_id: None,
            approval_mode: new.approval_mode,
            model: new.model,
            thinking_level: new.thinking_level,
            created_by: new.created_by,
            created_at: now_ms(),
            archived_at: None,
            workspace_ids: new.workspace_ids,
            tags: new.tags,
        };
        if let Err(error) = self.store.insert_thread(&thread) {
            // Best effort: no orphan directories for a thread that does not exist.
            let _ = std::fs::remove_dir_all(self.data.thread(&thread.id));
            return Err(error.into());
        }
        self.get(&thread.id)
    }

    pub fn get(&self, id: &str) -> Result<Thread> {
        Ok(self.store.thread(id)?)
    }

    /// Every thread, archived ones included, oldest first.
    pub fn list(&self) -> Result<Vec<Thread>> {
        Ok(self.store.threads()?)
    }

    /// Paths of the attached workspaces in order, for `--add-dir` (DMN-030).
    pub fn workspace_paths(&self, id: &str) -> Result<Vec<PathBuf>> {
        self.get(id)?
            .workspace_ids
            .iter()
            .map(|workspace_id| self.live_workspace_path(workspace_id))
            .collect()
    }

    /// Attaches a workspace at `position` (default: last); an attached one is moved there. Sharing a workspace
    /// between threads is allowed (DMN-027).
    pub fn attach(
        &self,
        id: &str,
        workspace_id: &str,
        position: Option<usize>,
    ) -> Result<Attachment> {
        let thread = self.get(id)?;
        let path = self.live_workspace_path(workspace_id)?;
        let mut ids = thread.workspace_ids;
        let current = ids.iter().position(|attached| attached == workspace_id);
        if let Some(index) = current {
            ids.remove(index);
        }
        let index = position.or(current).unwrap_or(ids.len()).min(ids.len());
        ids.insert(index, workspace_id.to_owned());
        self.store.set_thread_workspaces(id, &ids)?;
        Ok(Attachment {
            thread: self.get(id)?,
            change: current.is_none().then_some(DirChange::Added(path)),
        })
    }

    /// Detaches a workspace; detaching one that is not attached changes nothing.
    pub fn detach(&self, id: &str, workspace_id: &str) -> Result<Attachment> {
        let thread = self.get(id)?;
        let mut ids = thread.workspace_ids.clone();
        let Some(index) = ids.iter().position(|attached| attached == workspace_id) else {
            return Ok(Attachment {
                thread,
                change: None,
            });
        };
        ids.remove(index);
        let path = self.store.workspace(workspace_id)?.path;
        self.store.set_thread_workspaces(id, &ids)?;
        Ok(Attachment {
            thread: self.get(id)?,
            change: Some(DirChange::Removed(path)),
        })
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<Thread> {
        self.store.set_thread_title(id, title)?;
        self.get(id)
    }

    /// Replaces the tags (stored sorted, without duplicates).
    pub fn set_tags(&self, id: &str, tags: &[String]) -> Result<Thread> {
        self.store.set_thread_tags(id, tags)?;
        self.get(id)
    }

    pub fn archive(&self, id: &str) -> Result<Thread> {
        self.store.set_thread_archived_at(id, Some(now_ms()))?;
        self.get(id)
    }

    pub fn unarchive(&self, id: &str) -> Result<Thread> {
        self.store.set_thread_archived_at(id, None)?;
        self.get(id)
    }

    /// Records the omp session the thread is bound to (DMN-032, DMN-072).
    pub fn set_omp_session_id(&self, id: &str, session_id: &str) -> Result<()> {
        Ok(self.store.set_thread_omp_session_id(id, Some(session_id))?)
    }

    pub fn set_model(&self, id: &str, model: Option<&str>) -> Result<Thread> {
        self.store.set_thread_model(id, model)?;
        self.get(id)
    }

    pub fn set_thinking_level(&self, id: &str, level: Option<&str>) -> Result<Thread> {
        self.store.set_thread_thinking_level(id, level)?;
        self.get(id)
    }

    /// Deletes the thread and its directory (home, sessions, log). Workspaces are never removed implicitly. The
    /// caller makes sure no omp process runs for it.
    pub fn delete(&self, id: &str) -> Result<()> {
        self.store.delete_thread(id)?;
        match std::fs::remove_dir_all(self.data.thread(id)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }

    fn live_workspace_path(&self, workspace_id: &str) -> Result<PathBuf> {
        let ws = self.store.workspace(workspace_id)?;
        if ws.removed_at.is_some() {
            return Err(StoreError::NotFound {
                entity: "workspace",
                id: workspace_id.to_owned(),
            }
            .into());
        }
        Ok(ws.path)
    }
}
