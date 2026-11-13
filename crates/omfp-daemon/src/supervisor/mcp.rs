//! The MCP servers of a workspace and a thread (M13): the configured list (DMN-190), the check of each server by the
//! thread's running omp with `/mcp test` (DMN-191), and the reconnect, a restart of the idle process (DMN-192).
//!
//! omp loads MCP servers only at startup, so the daemon reloads a thread's omp by itself when the servers it would
//! load change: the configs' [`omp_mcp::fingerprint`] changed (DMN-201) or omp said a
//! server needs a new connection (`✓ Reauthorized`, `Added MCP server`, DMN-202). The restart waits for the turn to
//! settle (DMN-203); background work (omp jobs, subagents, a pending dialog, a live session) holds it back and marks
//! the thread `mcp_reload_pending` until it ends or the user reconnects (DMN-204).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use omfp_core::wire::SubagentLifecycleStatus;
use omfp_core::{JobState, ProcessState};
use omfp_protocol::{McpServer, McpServers};
use omp_rpc::PromptCommand;
use tokio::sync::oneshot;
use tracing::{info, warn};

use super::{Actor, Result, Supervisor, SupervisorError, blocking, lock};
use crate::omp_mcp::{self, Check, ConfigFingerprint, Homes};

/// The daemon's `/mcp test` of server `name` waiting for its `command_output` (DMN-200).
pub(super) struct PendingCheck {
    name: String,
    answer: oneshot::Sender<Check>,
}

/// Whether a thread's omp should connect anew to load new MCP tools (DMN-201..204).
#[derive(Debug, Default)]
pub(super) struct Reload {
    /// The workspace folder the running omp read its configs from and their fingerprint then.
    loaded: Option<(PathBuf, ConfigFingerprint)>,
    /// A reload is due.
    wanted: bool,
    /// Background work holds the due reload back (the thread's `mcp_reload_pending`).
    blocked: bool,
    /// A reload task is on its way.
    running: bool,
}

impl Reload {
    pub(super) fn want(&mut self) {
        self.wanted = true;
    }

    pub(super) fn blocked(&self) -> bool {
        self.blocked
    }

    /// A process read the configs (`Some`), or none runs any more (`None`): nothing is due.
    pub(super) fn loaded(&mut self, configs: Option<(PathBuf, ConfigFingerprint)>) {
        *self = Self {
            loaded: configs,
            ..Self::default()
        };
    }
}

/// What keeps a due reload from running now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hold {
    /// A turn runs (or the process is not up yet): the reload waits for it to settle.
    Turn,
    /// Background work would die with the process: the user decides (`mcp_reload_pending`).
    Background,
}

fn hold(actor: &Actor) -> Option<Hold> {
    if lock(&actor.voice).is_some() {
        return Some(Hold::Background);
    }
    actor.log.with_state(|state| {
        if state.process != ProcessState::Running || !state.settled {
            return Some(Hold::Turn);
        }
        let jobs = state.tools.iter().any(|run| {
            run.background
                .as_ref()
                .is_some_and(|job| job.state == JobState::Running)
        });
        let subagents = state.subagents.values().any(|subagent| {
            subagent
                .lifecycle
                .as_ref()
                .is_some_and(|lifecycle| lifecycle.status == SubagentLifecycleStatus::Started)
        });
        (jobs || subagents || !state.interactions.is_empty()).then_some(Hold::Background)
    })
}

/// How long the check's `command_output` may trail omp's answer to the prompt: omp writes the output first, so it is
/// only in transit through the event pump (DMN-191).
const OUTPUT_WAIT: Duration = Duration::from_secs(10);

impl Supervisor {
    fn homes(&self) -> Homes {
        Homes {
            home: self.inner.config.home.clone(),
            agent_dir: self.inner.config.agent_dir.clone(),
        }
    }

    /// The servers configured for workspace `workspace_id`, unchecked (PRT-110, DMN-190).
    pub async fn workspace_mcp_servers(&self, workspace_id: &str) -> Result<McpServers> {
        let workspaces = std::sync::Arc::clone(&self.inner.workspaces);
        let id = workspace_id.to_owned();
        let path = blocking(move || workspaces.get(&id)).await??.path;
        let servers = omp_mcp::configured(&path, &self.homes());
        Ok(self.listing(workspace_id, None, false, &path, servers))
    }

    /// The servers of the thread's workspace, each checkable one checked by the thread's omp when it runs (PRT-111,
    /// DMN-191). A stopped thread is not started: its servers run only with it.
    pub async fn thread_mcp_servers(&self, thread_id: &str) -> Result<McpServers> {
        self.thread_servers(thread_id, None).await
    }

    /// Reconnects the thread's MCP servers: omp 18.18.6 reconnects none over RPC (`/mcp reconnect` is TUI-only,
    /// `/mcp reload` does not reconnect), so the idle process restarts and connects them all again; then `name`, or
    /// every checkable server, is checked (PRT-112, DMN-192). A working thread is refused: a restart would abort it.
    pub async fn mcp_reconnect(&self, thread_id: &str, name: Option<&str>) -> Result<McpServers> {
        let actor = self.actor(thread_id).await?;
        if !actor.status().settled {
            return Err(SupervisorError::Busy);
        }
        self.restart(thread_id).await?;
        self.thread_servers(thread_id, name).await
    }

    /// The thread's servers; with `only`, the others are listed unchecked.
    async fn thread_servers(&self, thread_id: &str, only: Option<&str>) -> Result<McpServers> {
        let threads = std::sync::Arc::clone(&self.inner.threads);
        let workspaces = std::sync::Arc::clone(&self.inner.workspaces);
        let id = thread_id.to_owned();
        let (workspace_id, path) = blocking(move || {
            let thread = threads.get(&id)?;
            let workspace = workspaces.get(&thread.workspace_id)?;
            Ok::<_, SupervisorError>((thread.workspace_id, workspace.path))
        })
        .await??;
        let configured = omp_mcp::configured(&path, &self.homes());
        let running = self.is_running(thread_id);
        let mut servers = Vec::with_capacity(configured.len());
        for server in configured {
            let check =
                if running && server.checkable && only.is_none_or(|name| name == server.name) {
                    self.mcp_check(thread_id, &server.name).await?
                } else {
                    None
                };
            servers.push(omp_mcp::checked(server, check));
        }
        Ok(self.listing(&workspace_id, Some(thread_id), running, &path, servers))
    }

    fn listing(
        &self,
        workspace_id: &str,
        thread_id: Option<&str>,
        running: bool,
        path: &Path,
        servers: Vec<McpServer>,
    ) -> McpServers {
        McpServers {
            workspace_id: workspace_id.to_owned(),
            thread_id: thread_id.map(str::to_owned),
            running,
            project_config: omp_mcp::project_config(path).to_string_lossy().into_owned(),
            user_config: omp_mcp::user_config(&self.homes())
                .map(|path| path.to_string_lossy().into_owned()),
            servers,
        }
    }

    /// Checks server `name` with the thread's running omp: `/mcp test <name>` runs as a local command (no model turn)
    /// that connects anew, lists the tools and disconnects, and answers with one `command_output` frame (DMN-191).
    /// `None` when the thread does not run or omp sent no answer for this server. Checks of one thread run one at a
    /// time so their outputs cannot cross.
    async fn mcp_check(&self, thread_id: &str, name: &str) -> Result<Option<Check>> {
        let Some(actor) = self.lookup(thread_id) else {
            return Ok(None);
        };
        let _one_at_a_time = actor.local_commands.lock().await;
        let Some(process) = actor.process() else {
            return Ok(None);
        };
        let (answer, answered) = oneshot::channel();
        *lock(&actor.mcp_check) = Some(PendingCheck {
            name: name.to_owned(),
            answer,
        });
        actor.touch();
        let command = PromptCommand {
            message: format!("/mcp test {name}"),
            images: None,
            streaming_behavior: None,
        };
        let sent = blocking(move || process.prompt(command)).await;
        let check = match sent {
            Ok(Ok(_)) => tokio::time::timeout(OUTPUT_WAIT, answered)
                .await
                .ok()
                .and_then(std::result::Result::ok),
            Ok(Err(error)) => {
                warn!(thread = %thread_id, server = %name, %error, "MCP check failed");
                Some(Check::Failed(format!(
                    "omp did not answer the check: {error}"
                )))
            }
            Err(error) => {
                lock(&actor.mcp_check).take();
                return Err(error);
            }
        };
        lock(&actor.mcp_check).take();
        Ok(check)
    }

    /// Hands a `command_output` that answers the daemon's pending `/mcp test` to it; `true` when it did, so the
    /// output stays out of the thread's transcript (DMN-200).
    pub(super) fn take_check_output(actor: &Actor, text: &str) -> bool {
        let mut pending = lock(&actor.mcp_check);
        let Some(check) = pending
            .as_ref()
            .and_then(|pending| omp_mcp::parse_check(&pending.name, text))
        else {
            return false;
        };
        if let Some(pending) = pending.take() {
            let _ = pending.answer.send(check);
        }
        true
    }

    /// Records the configs the thread's omp is about to read: nothing is due any more (DMN-201).
    pub(super) async fn loading_mcp_configs(&self, actor: &Actor, workspace: PathBuf) {
        let homes = self.homes();
        let folder = workspace.clone();
        match blocking(move || omp_mcp::fingerprint(&folder, &homes)).await {
            Ok(fingerprint) => lock(&actor.mcp_reload).loaded(Some((workspace, fingerprint))),
            Err(error) => warn!(thread = %actor.id, %error, "cannot read the MCP configs"),
        }
    }

    /// Compares the MCP configs of every running thread with the ones its omp read; a change makes a reload due
    /// (DMN-201). The workspace polls call it every round.
    pub async fn check_mcp_configs(&self) {
        let actors: Vec<Arc<Actor>> = lock(&self.inner.actors).values().cloned().collect();
        for actor in actors {
            let Some((workspace, _)) = lock(&actor.mcp_reload).loaded.clone() else {
                continue;
            };
            if actor.process().is_none() {
                continue;
            }
            let homes = self.homes();
            let Ok(now) = blocking(move || omp_mcp::fingerprint(&workspace, &homes)).await else {
                continue;
            };
            let changed = {
                let mut reload = lock(&actor.mcp_reload);
                match &mut reload.loaded {
                    Some((_, fingerprint)) if *fingerprint != now => {
                        *fingerprint = now;
                        reload.wanted = true;
                        true
                    }
                    _ => false,
                }
            };
            if changed {
                info!(thread = %actor.id, "MCP configs changed");
                self.reload_mcp_when_free(&actor);
            }
        }
    }

    /// Runs a due reload if nothing holds it back: at once when the thread is idle, after the turn when it works
    /// (DMN-203); background work marks the thread instead (DMN-204). Called after every event of the thread.
    pub(super) fn reload_mcp_when_free(&self, actor: &Actor) {
        {
            let reload = lock(&actor.mcp_reload);
            if !reload.wanted || reload.running {
                return;
            }
        }
        if actor.process().is_none() {
            // A stopped thread reads the configs when it starts again.
            lock(&actor.mcp_reload).loaded(None);
            return;
        }
        let hold = hold(actor);
        let mut reload = lock(&actor.mcp_reload);
        if !reload.wanted || reload.running {
            return;
        }
        let blocked = hold == Some(Hold::Background);
        let changed = reload.blocked != blocked;
        reload.blocked = blocked;
        if hold.is_none() {
            reload.running = true;
            let supervisor = self.clone();
            let id = actor.id.clone();
            tokio::spawn(async move { supervisor.reload_mcp(&id).await });
        }
        drop(reload);
        if changed {
            let _ = self.inner.changes.send(actor.id.clone());
        }
    }

    /// The reload task: once no prompt is on its way and no lifecycle operation runs, restarts the thread's omp if it
    /// is still free (same session, `--continue`, DMN-032).
    async fn reload_mcp(&self, id: &str) {
        let Some(actor) = self.lookup(id) else {
            return;
        };
        let quiet = actor.prompting.write().await;
        let lifecycle = actor.lifecycle.lock().await;
        if actor.process().is_none() {
            lock(&actor.mcp_reload).loaded(None);
            return;
        }
        if hold(&actor).is_some() {
            // Work started meanwhile: wait again.
            lock(&actor.mcp_reload).running = false;
            drop((lifecycle, quiet));
            self.reload_mcp_when_free(&actor);
            return;
        }
        info!(thread = %id, "restarting omp to load MCP tools");
        let restarted = match self.stop_locked(&actor).await {
            Ok(()) => self.start_locked(&actor).await.map(drop),
            Err(error) => Err(error),
        };
        if let Err(error) = restarted {
            warn!(thread = %id, %error, "restart for MCP tools failed");
            lock(&actor.mcp_reload).running = false;
        }
    }
}
