//! Agent supervision (02-daemon, "Agent supervision"): one actor per thread owns the thread's event log and, while
//! omp runs, its process. Lifecycle operations (start, stop, restart, attach, detach) are serialized per thread;
//! omp commands are forwarded concurrently (DMN-037); threads proceed in parallel.
//!
//! omp sits behind the [`AgentLauncher`] port (ARC-012). Time is tokio's clock, so component tests drive the idle
//! reaper with paused time.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use omp_rpc::{
    AnswersUiResponse, CancelUiResponse, ConfirmUiResponse, ExtensionUiRequest,
    ExtensionUiResponse, PromptCommand, ValueUiResponse,
};
use pmo_core::wire::LitTrue;
use pmo_core::{ProcessState, Resolution, ThreadEvent, ThreadState, validate_ask_answers};
use pmo_protocol::InteractionResponse;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{info, warn};

use crate::agent::{AgentLauncher, AgentProcess, Lease, OmpCall, PromptAdmission};
use crate::event_log::EventLog;
use crate::host_tools;
use crate::omp::{LaunchSpec, startup_events, thread_event};
use crate::paths::DataDir;
use crate::store::{Thread, new_id};
use crate::threads::{DirChange, ThreadError, Threads};

/// The event log of one thread: omp frames and daemon events folded by the shared reducer (ARC-006).
pub type ThreadLog = EventLog<ThreadEvent, ThreadState>;

/// How long shutdown waits for omp processes to stop (DMN-005).
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

/// Lines of `omp.log` reported with a crash (DMN-035).
const STDERR_TAIL_LINES: usize = 20;

/// Supervisor settings.
#[derive(Debug, Clone)]
pub struct SupervisorConfig {
    /// omp executable (DMN-006).
    pub omp_program: PathBuf,
    /// Environment overrides for every omp process (tests isolate omp with them).
    pub omp_env: Vec<(OsString, Option<OsString>)>,
    /// Maximum concurrent omp processes (DMN-038).
    pub max_processes: usize,
    /// Idle, settled, unwatched processes are stopped after this long (DMN-034).
    pub idle_timeout: Duration,
    /// Events kept per thread for catch-up (DMN-041).
    pub event_capacity: usize,
    /// How far a subscriber may fall behind before it must resynchronize (PRT-023).
    pub live_buffer: usize,
}

impl SupervisorConfig {
    /// Spec defaults: 8 processes, 15 minutes idle timeout, 2000 events kept.
    #[must_use]
    pub fn new(omp_program: impl Into<PathBuf>) -> Self {
        Self {
            omp_program: omp_program.into(),
            omp_env: Vec::new(),
            max_processes: 8,
            idle_timeout: Duration::from_mins(15),
            event_capacity: 2000,
            live_buffer: 4096,
        }
    }
}

/// Why a supervisor operation failed.
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("at most {0} omp processes may run at once; stop another thread first")]
    LimitReached(usize),
    #[error(
        "omp session {session_id} is open in another omp process (e.g. in a terminal); close it there first"
    )]
    SessionLeased { session_id: String },
    #[error("omp failed to start: {0}")]
    Start(String),
    #[error("omp `{command}` failed: {message}")]
    Omp {
        command: String,
        message: String,
        /// omp's own code (DMN-071).
        code: Option<String>,
    },
    #[error("the omp process is gone: {0}")]
    ProcessGone(String),
    #[error("interaction {0} is not pending (already answered or cancelled)")]
    AlreadyResolved(String),
    #[error("{0}")]
    InvalidResponse(String),
    #[error("the daemon is shutting down")]
    ShuttingDown,
    #[error(transparent)]
    Thread(#[from] ThreadError),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T, E = SupervisorError> = std::result::Result<T, E>;

impl From<omp_rpc::Error> for SupervisorError {
    fn from(error: omp_rpc::Error) -> Self {
        match error {
            omp_rpc::Error::Command {
                command,
                error,
                code,
            } => Self::Omp {
                command,
                message: error,
                code,
            },
            omp_rpc::Error::Timeout { command } => Self::Omp {
                command,
                message: "no response in time".into(),
                code: None,
            },
            other @ (omp_rpc::Error::Closed
            | omp_rpc::Error::Io(_)
            | omp_rpc::Error::Protocol(_)) => Self::ProcessGone(other.to_string()),
            other => Self::Omp {
                command: String::new(),
                message: other.to_string(),
                code: None,
            },
        }
    }
}

/// What the thread list shows about a thread's process (`ThreadSummary`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadStatus {
    pub process: ProcessState,
    pub settled: bool,
    pub needs_attention: bool,
}

impl Default for ThreadStatus {
    fn default() -> Self {
        Self {
            process: ProcessState::Stopped,
            settled: true,
            needs_attention: false,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[allow(clippy::trivially_copy_pass_by_ref)] // `EventLog` takes `fn(&mut S, &E)`.
fn reduce(state: &mut ThreadState, event: &ThreadEvent) {
    state.apply(event);
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| SupervisorError::Internal(error.to_string()))
}

/// A running process of a thread.
struct Live {
    process: Arc<dyn AgentProcess>,
    generation: u64,
    /// Becomes `true` once every event of the process has been pushed.
    ended: watch::Receiver<bool>,
}

/// One thread: its log, its process and who watches it.
struct Actor {
    id: String,
    log: Arc<ThreadLog>,
    /// Serializes lifecycle operations (DMN-037).
    lifecycle: tokio::sync::Mutex<()>,
    live: Mutex<Option<Live>>,
    /// Makes check-forward-record of an answer atomic (DMN-051).
    answers: Mutex<()>,
    watchers: AtomicUsize,
    last_activity: Mutex<Instant>,
}

impl Actor {
    fn new(id: &str, config: &SupervisorConfig) -> Self {
        Self {
            id: id.to_owned(),
            log: Arc::new(EventLog::new(
                new_id(),
                ThreadState::default(),
                reduce,
                config.event_capacity,
                config.live_buffer,
            )),
            lifecycle: tokio::sync::Mutex::new(()),
            live: Mutex::new(None),
            answers: Mutex::new(()),
            watchers: AtomicUsize::new(0),
            last_activity: Mutex::new(Instant::now()),
        }
    }

    fn process(&self) -> Option<Arc<dyn AgentProcess>> {
        lock(&self.live)
            .as_ref()
            .map(|live| Arc::clone(&live.process))
    }

    fn status(&self) -> ThreadStatus {
        self.log.with_state(|state| ThreadStatus {
            process: state.process.clone(),
            settled: state.settled,
            needs_attention: !state.interactions.is_empty(),
        })
    }

    fn touch(&self) {
        *lock(&self.last_activity) = Instant::now();
    }

    /// Running, settled, without pending requests and unwatched since at least `timeout` (DMN-034).
    fn is_idle(&self, timeout: Duration, now: Instant) -> bool {
        lock(&self.live).is_some()
            && self.watchers.load(Ordering::SeqCst) == 0
            && self.log.with_state(|state| {
                state.process == ProcessState::Running
                    && state.settled
                    && state.interactions.is_empty()
            })
            && now.saturating_duration_since(*lock(&self.last_activity)) >= timeout
    }
}

struct Inner {
    launcher: Arc<dyn AgentLauncher>,
    threads: Arc<Threads>,
    data: DataDir,
    config: SupervisorConfig,
    actors: Mutex<HashMap<String, Arc<Actor>>>,
    /// Processes running or starting (DMN-038).
    running: AtomicUsize,
    generations: AtomicU64,
    /// Ids of threads whose [`ThreadStatus`] changed.
    changes: broadcast::Sender<String>,
    shutting_down: AtomicBool,
}

/// The agent supervisor. Cheap to clone.
#[derive(Clone)]
pub struct Supervisor {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Supervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Supervisor")
            .field("config", &self.inner.config)
            .finish_non_exhaustive()
    }
}

/// Counts a client watching a thread while alive (PRT-025).
pub struct WatchGuard {
    actor: Arc<Actor>,
}

impl std::fmt::Debug for WatchGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WatchGuard").field(&self.actor.id).finish()
    }
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        self.actor.watchers.fetch_sub(1, Ordering::SeqCst);
        // The idle timeout counts from the moment the last client left.
        self.actor.touch();
    }
}

impl Supervisor {
    #[must_use]
    pub fn new(
        launcher: Arc<dyn AgentLauncher>,
        threads: Arc<Threads>,
        data: DataDir,
        config: SupervisorConfig,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                launcher,
                threads,
                data,
                config,
                actors: Mutex::new(HashMap::new()),
                running: AtomicUsize::new(0),
                generations: AtomicU64::new(0),
                changes: broadcast::channel(256).0,
                shutting_down: AtomicBool::new(false),
            }),
        }
    }

    /// Ids of threads whose process state, settledness or pending requests changed.
    #[must_use]
    pub fn subscribe_changes(&self) -> broadcast::Receiver<String> {
        self.inner.changes.subscribe()
    }

    /// The event log of a thread (PRT-020..022).
    pub async fn thread_log(&self, id: &str) -> Result<Arc<ThreadLog>> {
        Ok(Arc::clone(&self.actor(id).await?.log))
    }

    /// Marks the thread as watched until the guard drops (PRT-025).
    pub async fn watch(&self, id: &str) -> Result<WatchGuard> {
        let actor = self.actor(id).await?;
        actor.watchers.fetch_add(1, Ordering::SeqCst);
        Ok(WatchGuard { actor })
    }

    /// Number of clients watching the thread (PRT-025).
    #[must_use]
    pub fn watchers(&self, id: &str) -> usize {
        self.lookup(id)
            .map_or(0, |actor| actor.watchers.load(Ordering::SeqCst))
    }

    /// Whether the thread has a live omp process.
    #[must_use]
    pub fn is_running(&self, id: &str) -> bool {
        self.lookup(id)
            .is_some_and(|actor| lock(&actor.live).is_some())
    }

    /// What the thread list shows about the thread.
    #[must_use]
    pub fn status(&self, id: &str) -> ThreadStatus {
        self.lookup(id)
            .map_or_else(ThreadStatus::default, |actor| actor.status())
    }

    /// Starts omp for the thread if it is not running.
    pub async fn start(&self, id: &str) -> Result<()> {
        let actor = self.actor(id).await?;
        self.ensure_running(&actor).await.map(drop)
    }

    /// Stops the thread's omp process, if any.
    pub async fn stop(&self, id: &str) -> Result<()> {
        let actor = self.actor(id).await?;
        let _lifecycle = actor.lifecycle.lock().await;
        self.stop_locked(&actor).await
    }

    /// Stops and starts the thread's process; the history reloads from the session (DMN-032).
    pub async fn restart(&self, id: &str) -> Result<()> {
        let actor = self.actor(id).await?;
        let _lifecycle = actor.lifecycle.lock().await;
        self.stop_locked(&actor).await?;
        self.start_locked(&actor).await.map(drop)
    }

    /// Stops the thread and forgets its actor (the thread was deleted).
    pub async fn forget(&self, id: &str) -> Result<()> {
        let Some(actor) = self.lookup(id) else {
            return Ok(());
        };
        {
            let _lifecycle = actor.lifecycle.lock().await;
            self.stop_locked(&actor).await?;
        }
        lock(&self.inner.actors).remove(id);
        Ok(())
    }

    /// Sends a prompt, starting omp first when needed (DMN-070); returns at admission (PRT-030).
    pub async fn prompt(&self, id: &str, command: PromptCommand) -> Result<PromptAdmission> {
        let actor = self.actor(id).await?;
        let process = self.ensure_running(&actor).await?;
        actor.touch();
        let admission = blocking(move || process.prompt(command)).await??;
        actor.touch();
        Ok(admission)
    }

    /// Forwards an omp command, starting omp first when needed (DMN-070). Commands do not wait for each other
    /// (DMN-037); omp's error codes are kept (DMN-071).
    pub async fn call(&self, id: &str, call: OmpCall) -> Result<Value> {
        let actor = self.actor(id).await?;
        let process = self.ensure_running(&actor).await?;
        actor.touch();
        let fork = matches!(call, OmpCall::Fork(_));
        let forwarded = Arc::clone(&process);
        let result = blocking(move || forwarded.call(call)).await??;
        actor.touch();
        if fork {
            self.refresh_session_id(id, process).await?;
        }
        Ok(result)
    }

    /// A fork moves the process to a new session file: re-read it and store its id (DMN-072).
    async fn refresh_session_id(&self, id: &str, process: Arc<dyn AgentProcess>) -> Result<()> {
        let state = blocking(move || process.call(OmpCall::State)).await??;
        let Some(session_id) = state.get("sessionId").and_then(Value::as_str) else {
            return Err(SupervisorError::Internal(
                "get_state returned no sessionId".into(),
            ));
        };
        let threads = Arc::clone(&self.inner.threads);
        let (id, session_id) = (id.to_owned(), session_id.to_owned());
        blocking(move || threads.set_omp_session_id(&id, &session_id)).await??;
        Ok(())
    }

    /// Answers a pending interactive request: the first valid answer is forwarded and recorded, later ones get
    /// `already_resolved` (DMN-051); `ask` answers are validated first (DMN-054).
    pub async fn respond(
        &self,
        id: &str,
        interaction_id: &str,
        response: InteractionResponse,
        client: &str,
    ) -> Result<()> {
        let actor = self.actor(id).await?;
        let _answers = lock(&actor.answers);
        let request = actor.log.with_state(|state| {
            state
                .interactions
                .iter()
                .find(|request| dialog_id(request) == Some(interaction_id))
                .cloned()
        });
        let (Some(request), Some(process)) = (request, actor.process()) else {
            return Err(SupervisorError::AlreadyResolved(interaction_id.to_owned()));
        };
        let reply = ui_response(&request, interaction_id, response)
            .map_err(SupervisorError::InvalidResponse)?;
        // `respond` only queues the frame, so holding the answer lock across it is cheap.
        process.respond(reply)?;
        self.push(
            &actor,
            ThreadEvent::InteractionResolved {
                id: interaction_id.to_owned(),
                resolution: Resolution::Client {
                    client: client.to_owned(),
                },
            },
        );
        Ok(())
    }

    /// Attaches a workspace; a running process gets `/add-dir` (DMN-029). The store is updated first.
    pub async fn attach(
        &self,
        id: &str,
        workspace_id: &str,
        position: Option<usize>,
    ) -> Result<Thread> {
        let actor = self.actor(id).await?;
        let _lifecycle = actor.lifecycle.lock().await;
        let threads = Arc::clone(&self.inner.threads);
        let (thread_id, workspace_id) = (id.to_owned(), workspace_id.to_owned());
        let attachment =
            blocking(move || threads.attach(&thread_id, &workspace_id, position)).await??;
        self.apply_dir_change(&actor, attachment.change).await?;
        Ok(attachment.thread)
    }

    /// Detaches a workspace; a running process gets `/remove-dir` (DMN-029). The store is updated first.
    pub async fn detach(&self, id: &str, workspace_id: &str) -> Result<Thread> {
        let actor = self.actor(id).await?;
        let _lifecycle = actor.lifecycle.lock().await;
        let threads = Arc::clone(&self.inner.threads);
        let (thread_id, workspace_id) = (id.to_owned(), workspace_id.to_owned());
        let attachment = blocking(move || threads.detach(&thread_id, &workspace_id)).await??;
        self.apply_dir_change(&actor, attachment.change).await?;
        Ok(attachment.thread)
    }

    async fn apply_dir_change(&self, actor: &Actor, change: Option<DirChange>) -> Result<()> {
        let (Some(change), Some(process)) = (change, actor.process()) else {
            return Ok(());
        };
        let message = match change {
            DirChange::Added(path) => format!("/add-dir {}", path.display()),
            DirChange::Removed(path) => format!("/remove-dir {}", path.display()),
        };
        let command = PromptCommand {
            message,
            images: None,
            streaming_behavior: None,
        };
        blocking(move || process.prompt(command)).await??;
        actor.touch();
        Ok(())
    }

    /// Stops every process (DMN-005): closing the `omp-rpc` client closes omp's stdin, then terminates its process
    /// group. Gives up waiting after [`SHUTDOWN_GRACE`]. Later starts are refused.
    pub async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::SeqCst);
        let actors: Vec<Arc<Actor>> = lock(&self.inner.actors).values().cloned().collect();
        let stops = actors.iter().map(|actor| async move {
            let _lifecycle = actor.lifecycle.lock().await;
            if let Err(error) = self.stop_locked(actor).await {
                warn!(thread = %actor.id, %error, "stopping omp failed");
            }
        });
        if tokio::time::timeout(SHUTDOWN_GRACE, futures::future::join_all(stops))
            .await
            .is_err()
        {
            warn!("omp processes did not stop within {SHUTDOWN_GRACE:?}");
        }
    }

    /// Stops idle threads every so often (DMN-034) until the supervisor is dropped or shut down.
    #[must_use]
    pub fn spawn_reaper(&self) -> JoinHandle<()> {
        let inner = Arc::downgrade(&self.inner);
        let period = (self.inner.config.idle_timeout / 4)
            .clamp(Duration::from_secs(1), Duration::from_mins(1));
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(period);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let Some(supervisor) = Weak::upgrade(&inner).map(|inner| Self { inner }) else {
                    break;
                };
                if supervisor.inner.shutting_down.load(Ordering::SeqCst) {
                    break;
                }
                supervisor.reap_idle().await;
            }
        })
    }

    async fn reap_idle(&self) {
        let timeout = self.inner.config.idle_timeout;
        let actors: Vec<Arc<Actor>> = lock(&self.inner.actors).values().cloned().collect();
        for actor in actors {
            if !actor.is_idle(timeout, Instant::now()) {
                continue;
            }
            let Ok(_lifecycle) = actor.lifecycle.try_lock() else {
                continue;
            };
            if actor.is_idle(timeout, Instant::now()) {
                info!(thread = %actor.id, "stopping idle omp process");
                if let Err(error) = self.stop_locked(&actor).await {
                    warn!(thread = %actor.id, %error, "stopping idle omp failed");
                }
            }
        }
    }

    fn lookup(&self, id: &str) -> Option<Arc<Actor>> {
        lock(&self.inner.actors).get(id).cloned()
    }

    async fn actor(&self, id: &str) -> Result<Arc<Actor>> {
        if let Some(actor) = self.lookup(id) {
            return Ok(actor);
        }
        let threads = Arc::clone(&self.inner.threads);
        let thread_id = id.to_owned();
        blocking(move || threads.get(&thread_id)).await??;
        Ok(Arc::clone(
            lock(&self.inner.actors)
                .entry(id.to_owned())
                .or_insert_with(|| Arc::new(Actor::new(id, &self.inner.config))),
        ))
    }

    fn push(&self, actor: &Actor, event: ThreadEvent) {
        let before = actor.status();
        actor.log.push(event);
        actor.touch();
        if actor.status() != before {
            let _ = self.inner.changes.send(actor.id.clone());
        }
    }

    async fn ensure_running(&self, actor: &Arc<Actor>) -> Result<Arc<dyn AgentProcess>> {
        if let Some(process) = actor.process() {
            return Ok(process);
        }
        let _lifecycle = actor.lifecycle.lock().await;
        if let Some(process) = actor.process() {
            return Ok(process);
        }
        self.start_locked(actor).await
    }

    /// Starts omp; the caller holds the lifecycle lock.
    async fn start_locked(&self, actor: &Arc<Actor>) -> Result<Arc<dyn AgentProcess>> {
        if self.inner.shutting_down.load(Ordering::SeqCst) {
            return Err(SupervisorError::ShuttingDown);
        }
        let max = self.inner.config.max_processes;
        self.inner
            .running
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |running| {
                (running < max).then_some(running + 1)
            })
            .map_err(|_| SupervisorError::LimitReached(max))?;
        let started = self.launch(actor).await;
        if started.is_err() {
            self.inner.running.fetch_sub(1, Ordering::SeqCst);
        }
        started
    }

    async fn launch(&self, actor: &Arc<Actor>) -> Result<Arc<dyn AgentProcess>> {
        let id = actor.id.clone();
        let threads = Arc::clone(&self.inner.threads);
        let (thread, add_dirs) = blocking(move || {
            let thread = threads.get(&id)?;
            let add_dirs = threads.workspace_paths(&id)?;
            Ok::<_, ThreadError>((thread, add_dirs))
        })
        .await??;
        let data = &self.inner.data;
        let spec = LaunchSpec {
            program: self.inner.config.omp_program.clone(),
            cwd: std::path::absolute(data.thread_home(&actor.id))
                .unwrap_or_else(|_| data.thread_home(&actor.id)),
            add_dirs,
            session_dir: thread.session_dir.clone(),
            model: thread.model.clone(),
            approval_mode: thread.approval_mode.clone(),
            stderr_log: data.thread_log(&actor.id),
            env: self.inner.config.omp_env.clone(),
        };
        self.push(
            actor,
            ThreadEvent::Process {
                state: ProcessState::Starting,
            },
        );

        if let Some(session_id) = thread.omp_session_id.clone() {
            let launcher = Arc::clone(&self.inner.launcher);
            let probe = spec.clone();
            let lease = blocking(move || launcher.session_lease(&probe, &session_id)).await?;
            match lease {
                Ok(Lease::Held) => {
                    let error = SupervisorError::SessionLeased {
                        session_id: thread.omp_session_id.unwrap_or_default(),
                    };
                    self.fail(actor, error.to_string(), String::new());
                    return Err(error);
                }
                Ok(Lease::Free) => {}
                Ok(Lease::Unknown) => {
                    warn!(thread = %actor.id, "omp could not probe the session lease; starting anyway");
                }
                Err(error) => {
                    self.fail(actor, error.to_string(), String::new());
                    return Err(SupervisorError::Start(error.to_string()));
                }
            }
        }

        let log = Arc::clone(&actor.log);
        let tools = host_tools::thread_tools(
            spec.cwd.clone(),
            Arc::new(move |event| {
                log.push(event);
            }),
        );
        let launcher = Arc::clone(&self.inner.launcher);
        let launch_spec = spec.clone();
        let launched = match blocking(move || launcher.launch(&launch_spec, tools)).await? {
            Ok(launched) => launched,
            Err(error) => {
                let path = spec.stderr_log.clone();
                let tail = blocking(move || stderr_tail(&path, STDERR_TAIL_LINES))
                    .await
                    .unwrap_or_default();
                self.fail(actor, error.to_string(), tail);
                return Err(SupervisorError::Start(error.to_string()));
            }
        };

        // A new process starts a new epoch: positions of the old one cannot be replayed (PRT-022).
        actor.log.new_epoch(new_id());
        for event in startup_events(&launched.startup) {
            self.push(actor, event);
        }
        let threads = Arc::clone(&self.inner.threads);
        let (id, session_id) = (
            actor.id.clone(),
            launched.startup.session.session_id.clone(),
        );
        if let Err(error) = blocking(move || threads.set_omp_session_id(&id, &session_id)).await? {
            warn!(thread = %actor.id, %error, "cannot store the omp session id");
        }

        let generation = self.inner.generations.fetch_add(1, Ordering::SeqCst);
        let (ended_tx, ended) = watch::channel(false);
        let process = launched.process;
        *lock(&actor.live) = Some(Live {
            process: Arc::clone(&process),
            generation,
            ended,
        });
        self.spawn_pump(Arc::clone(actor), launched.events, generation, ended_tx);
        info!(thread = %actor.id, "omp started");
        Ok(process)
    }

    fn fail(&self, actor: &Actor, message: String, stderr_tail: String) {
        self.push(
            actor,
            ThreadEvent::Process {
                state: ProcessState::Failed {
                    message,
                    exit_code: None,
                    stderr_tail,
                },
            },
        );
    }

    /// Forwards omp's frames into the log: a thread reads the blocking receiver into a channel, a task pushes them
    /// (DMN-040). When the stream ends without a requested stop, the thread fails (DMN-035).
    fn spawn_pump(
        &self,
        actor: Arc<Actor>,
        events: std::sync::mpsc::Receiver<omp_rpc::Event>,
        generation: u64,
        ended: watch::Sender<bool>,
    ) {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        std::thread::spawn(move || {
            for event in events {
                if sender.send(thread_event(event)).is_err() {
                    break;
                }
            }
        });
        let supervisor = self.clone();
        tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                supervisor.push(&actor, event);
            }
            supervisor.process_ended(&actor, generation).await;
            let _ = ended.send(true);
        });
    }

    async fn process_ended(&self, actor: &Actor, generation: u64) {
        let crashed = {
            let mut live = lock(&actor.live);
            if live
                .as_ref()
                .is_some_and(|live| live.generation == generation)
            {
                live.take()
            } else {
                None
            }
        };
        let Some(crashed) = crashed else {
            return;
        };
        self.inner.running.fetch_sub(1, Ordering::SeqCst);
        let path = self.inner.data.thread_log(&actor.id);
        let process = crashed.process;
        let tail = blocking(move || {
            drop(process);
            stderr_tail(&path, STDERR_TAIL_LINES)
        })
        .await
        .unwrap_or_default();
        warn!(thread = %actor.id, "omp exited unexpectedly");
        self.fail(actor, "omp exited unexpectedly".into(), tail);
    }

    /// Stops the process; the caller holds the lifecycle lock.
    async fn stop_locked(&self, actor: &Actor) -> Result<()> {
        let Some(Live {
            process, mut ended, ..
        }) = lock(&actor.live).take()
        else {
            return Ok(());
        };
        blocking(move || drop(process)).await?;
        let _ = ended.wait_for(|ended| *ended).await;
        self.inner.running.fetch_sub(1, Ordering::SeqCst);
        self.push(
            actor,
            ThreadEvent::Process {
                state: ProcessState::Stopped,
            },
        );
        info!(thread = %actor.id, "omp stopped");
        Ok(())
    }
}

/// The id of a dialog request; `None` for presentation-only methods.
fn dialog_id(request: &ExtensionUiRequest) -> Option<&str> {
    match request {
        ExtensionUiRequest::Select(select) => Some(&select.id),
        ExtensionUiRequest::Confirm(confirm) => Some(&confirm.id),
        ExtensionUiRequest::Input(input) => Some(&input.id),
        ExtensionUiRequest::Editor(editor) => Some(&editor.id),
        ExtensionUiRequest::Ask(ask) => Some(&ask.id),
        _ => None,
    }
}

/// The omp answer for `request`, checked against its kind: `select` takes one of its options, `confirm` a boolean,
/// `ask` answers in omp's shape (DMN-054); any request can be cancelled.
fn ui_response(
    request: &ExtensionUiRequest,
    id: &str,
    response: InteractionResponse,
) -> std::result::Result<ExtensionUiResponse, String> {
    let id = id.to_owned();
    match (request, response) {
        (_, InteractionResponse::Cancelled) => {
            Ok(ExtensionUiResponse::CancelUiResponse(CancelUiResponse {
                id,
                cancelled: LitTrue,
                timed_out: None,
            }))
        }
        (ExtensionUiRequest::Select(select), InteractionResponse::Value(value)) => {
            if select.options.contains(&value) {
                Ok(ExtensionUiResponse::ValueUiResponse(ValueUiResponse {
                    id,
                    value,
                }))
            } else {
                Err(format!(
                    "{value:?} is not one of the options {:?}",
                    select.options
                ))
            }
        }
        (
            ExtensionUiRequest::Input(_) | ExtensionUiRequest::Editor(_),
            InteractionResponse::Value(value),
        ) => Ok(ExtensionUiResponse::ValueUiResponse(ValueUiResponse {
            id,
            value,
        })),
        (ExtensionUiRequest::Confirm(_), InteractionResponse::Confirmed(confirmed)) => {
            Ok(ExtensionUiResponse::ConfirmUiResponse(ConfirmUiResponse {
                id,
                confirmed,
            }))
        }
        (ExtensionUiRequest::Ask(ask), InteractionResponse::Answers(answers)) => {
            validate_ask_answers(ask, &answers)?;
            Ok(ExtensionUiResponse::AnswersUiResponse(AnswersUiResponse {
                id,
                answers,
            }))
        }
        (_, response) => Err(format!(
            "{} cannot answer this request",
            match response {
                InteractionResponse::Value(_) => "a value",
                InteractionResponse::Confirmed(_) => "a confirmation",
                InteractionResponse::Answers(_) => "ask answers",
                InteractionResponse::Cancelled => "a cancellation",
            }
        )),
    }
}

/// The last `lines` lines of omp's stderr log (at most its last 16 KiB).
fn stderr_tail(path: &Path, lines: usize) -> String {
    const MAX_BYTES: u64 = 16 * 1024;
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let length = file.metadata().map_or(0, |metadata| metadata.len());
    if file
        .seek(SeekFrom::Start(length.saturating_sub(MAX_BYTES)))
        .is_err()
    {
        return String::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

#[cfg(test)]
mod tests;
