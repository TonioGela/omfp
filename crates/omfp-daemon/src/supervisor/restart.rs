//! Restarting a thread's omp (DMN-227, DMN-228): one routine for `thread.restart`, `thread.mcp_reconnect` and the
//! restarts the daemon makes by itself, because omp loads MCP servers and lists a provider's models only at startup.
//!
//! A restart the daemon owes a thread carries its reasons (`RestartReason`: new MCP configs, a reauthorized server, a
//! login). It waits for the turn to settle (DMN-203); background work (omp jobs, subagents, a pending dialog, a live
//! session) holds it back and puts the reasons on the thread's summary (`restartPending`) until that work ends or the
//! user restarts the thread (DMN-204).

use std::sync::Arc;

use omfp_core::wire::SubagentLifecycleStatus;
use omfp_core::{JobState, ProcessState};
use omfp_protocol::RestartReason;
use tracing::{info, warn};

use super::{Actor, Result, Supervisor, SupervisorError, blocking, lock};
use crate::threads::ThreadError;

/// Who asks for a restart, which decides what it waits for (DMN-227).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restart {
    /// `thread.restart`: at once, whatever the thread does.
    User,
    /// `thread.mcp_reconnect`: at once when the session is settled, refused while a turn runs; background work does
    /// not hold it back (the user decided, DMN-204).
    Reconnect,
    /// The daemon's own, for the thread's due reasons: once the process runs, its session is settled and no
    /// background work runs (DMN-203, DMN-204).
    Due,
}

/// A restart the daemon owes a thread's running omp (DMN-203, DMN-204, DMN-228).
#[derive(Debug, Default)]
pub(super) struct DueRestart {
    /// Why, each reason once in the order they came; empty when none is due.
    reasons: Vec<RestartReason>,
    /// Background work holds it back.
    blocked: bool,
    /// A restart task is on its way.
    running: bool,
}

impl DueRestart {
    pub(super) fn want(&mut self, reason: RestartReason) {
        if !self.reasons.contains(&reason) {
            self.reasons.push(reason);
        }
    }

    /// The thread summary's `restartPending`: the reasons, while background work holds the restart back.
    pub(super) fn pending(&self) -> Vec<RestartReason> {
        if self.blocked {
            self.reasons.clone()
        } else {
            Vec::new()
        }
    }

    /// A process started (it loads everything anew) or none runs any more: nothing is due.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// What keeps a due restart from running now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hold {
    /// A turn runs (or the process is not up yet): the restart waits for it to settle.
    Turn,
    /// Background work would die with the process: the user decides (`restartPending`).
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

impl Supervisor {
    /// Stops and starts the thread's process as `restart` asks (DMN-227); the history reloads from the session
    /// (DMN-032). Cancellation-safe ([`Self::detached`]). [`Restart::Due`] restarts only for due reasons and only
    /// when the thread is free; it returns at once otherwise.
    pub async fn restart(&self, id: &str, restart: Restart) -> Result<()> {
        let id = id.to_owned();
        self.detached(move |supervisor| async move {
            let actor = supervisor.actor(&id).await?;
            supervisor.restart_locked(&actor, restart).await.map(drop)
        })
        .await
    }

    /// The one restart (DMN-227): no message on its way to omp (`prompting`, DMN-226) and no other lifecycle
    /// operation; then the guard of `restart`, checked under both locks. `false` when a due restart does not apply.
    async fn restart_locked(&self, actor: &Arc<Actor>, restart: Restart) -> Result<bool> {
        let _quiet = actor.prompting.write().await;
        let _lifecycle = actor.lifecycle.lock().await;
        let reasons = match restart {
            Restart::User => Vec::new(),
            Restart::Reconnect if !actor.status().settled => return Err(SupervisorError::Busy),
            Restart::Reconnect => lock(&actor.restart).reasons.clone(),
            Restart::Due => {
                let reasons = lock(&actor.restart).reasons.clone();
                if actor.process().is_none() {
                    lock(&actor.restart).clear();
                    return Ok(false);
                }
                if reasons.is_empty() || hold(actor).is_some() {
                    return Ok(false);
                }
                reasons
            }
        };
        info!(thread = %actor.id, ?restart, ?reasons, "restarting omp");
        self.stop_locked(actor).await?;
        self.start_locked(actor).await?;
        Ok(true)
    }

    /// Moves the thread to another workspace with `apply` (DMN-283), under the locks of the one restart routine:
    /// refused while a turn runs or a dialog waits ([`SupervisorError::BusyMove`]); when `apply` moved it and its omp
    /// runs, that omp starts again in the new folder at once (its roots follow at start, DMN-029), whatever background
    /// work it had (the user moved it); a stopped thread starts there next time. Cancellation-safe
    /// ([`Self::detached`]).
    pub async fn relocate(
        &self,
        id: &str,
        apply: impl FnOnce() -> Result<bool, ThreadError> + Send + 'static,
    ) -> Result<()> {
        let id = id.to_owned();
        self.detached(move |supervisor| async move {
            let actor = supervisor.actor(&id).await?;
            let _quiet = actor.prompting.write().await;
            let _lifecycle = actor.lifecycle.lock().await;
            let running = actor.process().is_some();
            if running && !actor.status().settled {
                return Err(SupervisorError::BusyMove);
            }
            if blocking(apply).await?? && running {
                info!(thread = %actor.id, "moved to another workspace: restarting omp");
                supervisor.stop_locked(&actor).await?;
                supervisor.start_locked(&actor).await?;
            }
            Ok(())
        })
        .await
    }

    /// Makes a restart for `reason` due and runs it when the thread is free (DMN-228).
    pub(super) fn restart_for(&self, actor: &Arc<Actor>, reason: RestartReason) {
        lock(&actor.restart).want(reason);
        self.restart_when_free(actor);
    }

    /// Runs a due restart if nothing holds it back: at once when the thread is idle, after the turn when it works
    /// (DMN-203); background work marks the thread instead (DMN-204). Called after every event of the thread.
    pub(super) fn restart_when_free(&self, actor: &Actor) {
        {
            let due = lock(&actor.restart);
            if due.reasons.is_empty() || due.running {
                return;
            }
        }
        if actor.process().is_none() {
            // A stopped thread loads everything when it starts again.
            lock(&actor.restart).clear();
            return;
        }
        let hold = hold(actor);
        let mut due = lock(&actor.restart);
        if due.reasons.is_empty() || due.running {
            return;
        }
        let blocked = hold == Some(Hold::Background);
        let changed = due.blocked != blocked;
        due.blocked = blocked;
        if hold.is_none() {
            due.running = true;
            let supervisor = self.clone();
            let id = actor.id.clone();
            tokio::spawn(async move { supervisor.run_due_restart(&id).await });
        }
        drop(due);
        if changed {
            let _ = self.inner.changes.send(actor.id.clone());
        }
    }

    /// The task of a due restart; work that started meanwhile makes it wait again.
    async fn run_due_restart(&self, id: &str) {
        let Some(actor) = self.lookup(id) else {
            return;
        };
        match self.restart_locked(&actor, Restart::Due).await {
            Ok(true) => {}
            Ok(false) => {
                lock(&actor.restart).running = false;
                self.restart_when_free(&actor);
            }
            Err(error) => {
                warn!(thread = %id, %error, "due restart failed");
                lock(&actor.restart).running = false;
            }
        }
    }
}
