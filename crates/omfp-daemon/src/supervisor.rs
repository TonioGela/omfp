//! Agent supervision (02-daemon, "Agent supervision"): one actor per thread owns the thread's event log and, while
//! omp runs, its process. Lifecycle operations (start, stop, restart, root sync) are serialized per thread; omp
//! commands are forwarded concurrently (DMN-037); threads proceed in parallel.
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

use omfp_core::wire::LitTrue;
use omfp_core::{
    LiveOwner, OpenFileRequest, ProcessState, Resolution, ThreadEvent, ThreadState,
    live_error_message, validate_ask_answers,
};
use omfp_protocol::InteractionResponse;
use omp_rpc::{
    AnswersUiResponse, CancelUiResponse, ConfirmUiResponse, ExtensionUiRequest,
    ExtensionUiResponse, LiveConnectCommand, LiveConnectResult, LiveMuteCommand, PromptCommand,
    RpcAgentEvent, RpcNotification, SetSessionNameCommand, ValueUiResponse,
};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;
use tracing::{info, warn};

use crate::agent::{AgentLauncher, AgentProcess, Lease, OmpCall, PromptAdmission};
use crate::conventions;
use crate::event_log::EventLog;
use crate::host_tools;
use crate::omp::{LaunchSpec, startup_events, thread_event};
use crate::paths::DataDir;
use crate::store::{Thread, TitleSource, new_id};
use crate::threads::{ThreadError, Threads};
use crate::titles::{self, TitleRequest};
use crate::widgets;
use crate::workspaces::{WorkspaceError, Workspaces};

/// The event log of one thread: omp frames and daemon events folded by the shared reducer (ARC-006).
pub type ThreadLog = EventLog<ThreadEvent, ThreadState>;

/// How long shutdown waits for omp processes to stop (DMN-005).
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

/// Lines of `omp.log` reported with a crash (DMN-035).
const STDERR_TAIL_LINES: usize = 20;

/// omp tools that never change files; their end needs no rescan (DMN-023). Unknown tools count as writers.
const READ_ONLY_TOOLS: &[&str] = &[
    "read",
    "grep",
    "glob",
    "find",
    "ast_grep",
    "web_search",
    "fetch",
    "todo",
    "ask",
    "lsp",
];

/// omp's `live_connect` failures that come before a session exists: no `live_end` follows them (S-09).
const LIVE_NOT_STARTED: &[&str] = &[
    "live_connect requires a non-empty sdpOffer",
    "End dictation before starting live voice",
    "Session is changing or shutting down",
    "A live session is already active",
];

/// omp's `live_mute` failure without a session.
const LIVE_NO_SESSION: &str = "No live session is active";

/// What `omfp_open_file` answers when no window shows its thread (DMN-060).
pub const NO_WINDOW: &str = "No omfp window shows this thread, so the file cannot be opened: give the user its path \
                             instead, or ask them to open the thread in omfp and try again.";

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
    /// omp's user agent directory, where the user's `APPEND_SYSTEM.md` lives (DMN-030).
    pub agent_dir: Option<PathBuf>,
    /// Generate thread titles with an omp print-mode run after the first message (DMN-073). Tests against the fake
    /// LLM turn it off so the extra model call does not take a scripted step.
    pub title_generation: bool,
    /// `<static-dir>/builtin-widgets/` when the daemon serves a bundle with built-in widgets: the `omfp://` host URIs
    /// serve their sources (DMN-062).
    pub builtin_widgets: Option<PathBuf>,
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
            agent_dir: None,
            title_generation: true,
            builtin_widgets: None,
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
    #[error("thread.live_connect needs a non-empty sdpOffer")]
    EmptyOffer,
    #[error("a live session is already running on this thread (started by {client})")]
    LiveActive { client: String },
    #[error("no live session is running on this thread")]
    NoLiveSession,
    /// A live voice failure, readable (DMN-104); `omp` is omp's own message.
    #[error("{message}")]
    Live { message: String, omp: String },
    #[error("the daemon is shutting down")]
    ShuttingDown,
    #[error(transparent)]
    Thread(#[from] ThreadError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
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

/// The live voice session of a thread, from `thread.live_connect` until it ends (DMN-101).
struct VoiceSlot {
    /// Tells a session from the next one on the same thread.
    id: u64,
    owner: LiveOwner,
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
    /// Connections watching the thread (PRT-025), one entry per subscription, the one the user last used on it last
    /// (DMN-060).
    watchers: Mutex<Vec<u64>>,
    last_activity: Mutex<Instant>,
    /// A root sync was asked while the agent was busy (omp refuses `/add-dir` while streaming); it runs once the
    /// session settles (DMN-029).
    roots_pending: AtomicBool,
    /// The first message named the thread already (DMN-073).
    titled: AtomicBool,
    /// The running title generation, aborted when the thread is forgotten.
    title_task: Mutex<Option<JoinHandle<()>>>,
    /// The live voice session (DMN-101).
    voice: Mutex<Option<VoiceSlot>>,
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
            watchers: Mutex::new(Vec::new()),
            last_activity: Mutex::new(Instant::now()),
            roots_pending: AtomicBool::new(false),
            titled: AtomicBool::new(false),
            title_task: Mutex::new(None),
            voice: Mutex::new(None),
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
            && lock(&self.watchers).is_empty()
            && self.log.with_state(|state| {
                state.process == ProcessState::Running
                    && state.settled
                    && state.interactions.is_empty()
            })
            && now.saturating_duration_since(*lock(&self.last_activity)) >= timeout
            && lock(&self.voice).is_none()
    }

    /// Frees the live session slot if it still holds session `id`; `true` when it did.
    fn release_voice(&self, id: u64) -> bool {
        let mut voice = lock(&self.voice);
        if voice.as_ref().is_some_and(|slot| slot.id == id) {
            voice.take();
            true
        } else {
            false
        }
    }
}

struct Inner {
    launcher: Arc<dyn AgentLauncher>,
    threads: Arc<Threads>,
    workspaces: Arc<Workspaces>,
    data: DataDir,
    config: SupervisorConfig,
    actors: Mutex<HashMap<String, Arc<Actor>>>,
    /// Processes running or starting (DMN-038).
    running: AtomicUsize,
    generations: AtomicU64,
    /// Numbers live sessions (DMN-101).
    voice_ids: AtomicU64,
    /// Ids of threads whose [`ThreadStatus`] changed.
    changes: broadcast::Sender<String>,
    /// Ids of threads that finished a tool execution (a rescan trigger, DMN-023).
    tool_ends: broadcast::Sender<String>,
    /// Clients watching each workspace (subscribers of `workspace/<id>`), which polls it (DMN-023).
    workspace_watchers: Mutex<HashMap<String, usize>>,
    /// Backs the widget host tools of every thread launched from now on (DMN-063, DMN-064); set by the server once
    /// its services exist, dropped at shutdown (the services hold the supervisor too).
    widget_host: Mutex<Option<Arc<dyn host_tools::WidgetHost>>>,
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
    connection: u64,
}

impl std::fmt::Debug for WatchGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WatchGuard").field(&self.actor.id).finish()
    }
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        let mut watchers = lock(&self.actor.watchers);
        if let Some(index) = watchers
            .iter()
            .rposition(|watcher| *watcher == self.connection)
        {
            watchers.remove(index);
        }
        drop(watchers);
        // The idle timeout counts from the moment the last client left.
        self.actor.touch();
    }
}

/// Counts a client watching a workspace while alive (DMN-023).
pub struct WorkspaceWatchGuard {
    inner: Arc<Inner>,
    id: String,
}

impl std::fmt::Debug for WorkspaceWatchGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WorkspaceWatchGuard")
            .field(&self.id)
            .finish()
    }
}

impl Drop for WorkspaceWatchGuard {
    fn drop(&mut self) {
        let mut watchers = lock(&self.inner.workspace_watchers);
        if let Some(count) = watchers.get_mut(&self.id) {
            *count -= 1;
            if *count == 0 {
                watchers.remove(&self.id);
            }
        }
    }
}

impl Supervisor {
    #[must_use]
    pub fn new(
        launcher: Arc<dyn AgentLauncher>,
        threads: Arc<Threads>,
        workspaces: Arc<Workspaces>,
        data: DataDir,
        config: SupervisorConfig,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                launcher,
                threads,
                workspaces,
                data,
                config,
                actors: Mutex::new(HashMap::new()),
                running: AtomicUsize::new(0),
                generations: AtomicU64::new(0),
                voice_ids: AtomicU64::new(0),
                changes: broadcast::channel(256).0,
                tool_ends: broadcast::channel(256).0,
                workspace_watchers: Mutex::new(HashMap::new()),
                widget_host: Mutex::new(None),
                shutting_down: AtomicBool::new(false),
            }),
        }
    }

    /// Gives the threads launched from now on the widget host tools (DMN-063, DMN-064).
    pub fn set_widget_host(&self, host: Arc<dyn host_tools::WidgetHost>) {
        *lock(&self.inner.widget_host) = Some(host);
    }

    /// Ids of threads whose agent finished a tool execution (`tool_execution_end`).
    #[must_use]
    pub fn subscribe_tool_ends(&self) -> broadcast::Receiver<String> {
        self.inner.tool_ends.subscribe()
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

    /// Marks the thread as watched by connection `connection` until the guard drops (PRT-025); it becomes the
    /// window the user last used on the thread (DMN-060).
    pub async fn watch(&self, id: &str, connection: u64) -> Result<WatchGuard> {
        let actor = self.actor(id).await?;
        lock(&actor.watchers).push(connection);
        Ok(WatchGuard { actor, connection })
    }

    /// Connection `connection` sent thread `id` something on the user's behalf: if it watches the thread, it becomes
    /// the window the user last used on it (DMN-060).
    pub fn used(&self, id: &str, connection: u64) {
        if let Some(actor) = self.lookup(id) {
            let mut watchers = lock(&actor.watchers);
            if let Some(index) = watchers.iter().rposition(|watcher| *watcher == connection) {
                let watcher = watchers.remove(index);
                watchers.push(watcher);
            }
        }
    }

    /// Number of clients watching the thread (PRT-025).
    #[must_use]
    pub fn watchers(&self, id: &str) -> usize {
        self.lookup(id)
            .map_or(0, |actor| lock(&actor.watchers).len())
    }

    /// Asks the window the user last used on thread `id` to open `path` at `line` (DMN-060): an `open_file` event
    /// naming its connection, which the window acts on. Without a window watching the thread the error tells the
    /// agent what to do instead.
    pub fn open_file(&self, id: &str, path: &str, line: Option<u64>) -> Result<(), String> {
        let actor = self.lookup(id).ok_or(NO_WINDOW)?;
        let connection = lock(&actor.watchers).last().copied().ok_or(NO_WINDOW)?;
        self.push(
            &actor,
            ThreadEvent::OpenFile(OpenFileRequest {
                id: new_id(),
                connection,
                path: path.to_owned(),
                line,
            }),
        );
        Ok(())
    }

    /// Whether the thread has a live omp process.
    #[must_use]
    pub fn is_running(&self, id: &str) -> bool {
        self.lookup(id)
            .is_some_and(|actor| lock(&actor.live).is_some())
    }

    /// Marks the workspace as watched until the guard drops (DMN-023).
    #[must_use]
    pub fn watch_workspace(&self, id: &str) -> WorkspaceWatchGuard {
        *lock(&self.inner.workspace_watchers)
            .entry(id.to_owned())
            .or_default() += 1;
        WorkspaceWatchGuard {
            inner: Arc::clone(&self.inner),
            id: id.to_owned(),
        }
    }

    /// Number of clients watching the workspace (DMN-023).
    #[must_use]
    pub fn workspace_watchers(&self, id: &str) -> usize {
        lock(&self.inner.workspace_watchers)
            .get(id)
            .copied()
            .unwrap_or(0)
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
        if let Some(task) = lock(&actor.title_task).take() {
            task.abort();
        }
        lock(&self.inner.actors).remove(id);
        Ok(())
    }

    /// Sends a prompt, starting omp first when needed (DMN-070); returns at admission (PRT-030). The thread's first
    /// message names it (DMN-073): a placeholder at once, a generated title later.
    pub async fn prompt(&self, id: &str, command: PromptCommand) -> Result<PromptAdmission> {
        let actor = self.actor(id).await?;
        let process = self.ensure_running(&actor).await?;
        self.roots_before_prompt(&actor).await;
        actor.touch();
        let message = command.message.clone();
        let admission = blocking(move || process.prompt(command)).await??;
        actor.touch();
        if admission.agent_invoked {
            self.name_from_first_message(&actor, message).await;
        }
        Ok(admission)
    }

    /// A root change made before the prompt reaches omp first (DMN-029): a sync still pending runs now if the session
    /// settled, and one in flight (it holds the lifecycle lock) finishes before the prompt is sent.
    async fn roots_before_prompt(&self, actor: &Actor) {
        if actor.roots_pending.load(Ordering::SeqCst) && actor.status().settled {
            if let Err(error) = self.sync_roots(&actor.id).await {
                warn!(thread = %actor.id, %error, "root sync before the prompt failed");
            }
        } else {
            drop(actor.lifecycle.lock().await);
        }
    }

    /// Gives an unnamed thread the placeholder of its first message and starts generating its title (DMN-073).
    async fn name_from_first_message(&self, actor: &Actor, message: String) {
        let placeholder = titles::placeholder(&message);
        if placeholder.is_empty() || actor.titled.load(Ordering::SeqCst) {
            return;
        }
        let threads = Arc::clone(&self.inner.threads);
        let workspaces = Arc::clone(&self.inner.workspaces);
        let id = actor.id.clone();
        let named = blocking(move || {
            let thread = threads.get(&id)?;
            if thread.title_source != TitleSource::Placeholder || !thread.title.is_empty() {
                return Ok::<_, SupervisorError>(None);
            }
            let cwd = workspaces.get(&thread.workspace_id)?.path;
            threads.replace_placeholder(&id, &placeholder, TitleSource::Placeholder)?;
            Ok(Some(cwd))
        })
        .await;
        let cwd = match named {
            Ok(Ok(Some(cwd))) => cwd,
            Ok(Ok(None)) => return,
            Ok(Err(error)) | Err(error) => {
                warn!(thread = %actor.id, %error, "cannot store the placeholder title");
                return;
            }
        };
        if actor.titled.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.inner.changes.send(actor.id.clone());
        if self.inner.config.title_generation {
            self.spawn_title_generation(actor, cwd, message);
        }
    }

    /// Runs omp in print mode for the thread's title; the title replaces the placeholder unless a title was set
    /// meanwhile, and names the omp session (DMN-073).
    fn spawn_title_generation(&self, actor: &Actor, cwd: PathBuf, message: String) {
        let request = TitleRequest {
            program: self.inner.config.omp_program.clone(),
            cwd,
            env: self.inner.config.omp_env.clone(),
            message,
        };
        let supervisor = self.clone();
        let id = actor.id.clone();
        let task = tokio::spawn(async move {
            let launcher = Arc::clone(&supervisor.inner.launcher);
            let title = match blocking(move || launcher.generate_title(&request)).await {
                Ok(Ok(title)) => title,
                Ok(Err(error)) => {
                    warn!(thread = %id, %error, "title generation failed; the placeholder stays");
                    return;
                }
                Err(error) => {
                    warn!(thread = %id, %error, "title generation failed; the placeholder stays");
                    return;
                }
            };
            let threads = Arc::clone(&supervisor.inner.threads);
            let (thread_id, generated) = (id.clone(), title.clone());
            let stored = blocking(move || {
                threads.replace_placeholder(&thread_id, &generated, TitleSource::Generated)
            })
            .await;
            match stored {
                Ok(Ok(true)) => {
                    // Named first, so a client seeing the title on the index finds the session named too.
                    supervisor.name_session(&id, title).await;
                    let _ = supervisor.inner.changes.send(id.clone());
                }
                Ok(Ok(false)) => {}
                Ok(Err(error)) => warn!(thread = %id, %error, "cannot store the generated title"),
                Err(error) => warn!(thread = %id, %error, "cannot store the generated title"),
            }
        });
        *lock(&actor.title_task) = Some(task);
    }

    /// Waits until the thread's title generation, if any, is over.
    #[cfg(test)]
    async fn title_generated(&self, id: &str) {
        let task = self
            .lookup(id)
            .and_then(|actor| lock(&actor.title_task).take());
        if let Some(task) = task {
            let _ = task.await;
        }
    }

    /// Sets the thread's title and names its omp session after it (DMN-073). A [`TitleSource::Set`] title is never
    /// replaced by a generated one.
    pub async fn set_title(&self, id: &str, title: String, source: TitleSource) -> Result<Thread> {
        let threads = Arc::clone(&self.inner.threads);
        let (thread_id, stored) = (id.to_owned(), title.clone());
        let thread = blocking(move || threads.rename(&thread_id, &stored, source)).await??;
        self.name_session(id, title).await;
        let _ = self.inner.changes.send(id.to_owned());
        Ok(thread)
    }

    /// Sends `set_session_name` to the thread's omp when it runs (omp rejects an empty name); a stopped thread's
    /// session keeps its name until the next title change.
    async fn name_session(&self, id: &str, name: String) {
        let Some(process) = self.lookup(id).and_then(|actor| actor.process()) else {
            return;
        };
        if name.trim().is_empty() {
            return;
        }
        let call = OmpCall::SetSessionName(SetSessionNameCommand { name });
        match blocking(move || process.call(call)).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => warn!(thread = %id, %error, "cannot name the omp session"),
            Err(error) => warn!(thread = %id, %error, "cannot name the omp session"),
        }
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

    /// Starts a live voice session for `owner` on the client's peer (DMN-100): one per thread (DMN-101), recorded as a
    /// `live_session` event before omp gets the offer. Returns omp's SDP answer once the call is signaled.
    pub async fn live_connect(
        &self,
        id: &str,
        owner: LiveOwner,
        sdp_offer: String,
    ) -> Result<LiveConnectResult> {
        if sdp_offer.trim().is_empty() {
            return Err(SupervisorError::EmptyOffer);
        }
        let actor = self.actor(id).await?;
        let slot = self.inner.voice_ids.fetch_add(1, Ordering::SeqCst);
        {
            let mut voice = lock(&actor.voice);
            if let Some(active) = &*voice {
                return Err(SupervisorError::LiveActive {
                    client: active.owner.client.clone(),
                });
            }
            *voice = Some(VoiceSlot {
                id: slot,
                owner: owner.clone(),
            });
        }
        let process = match self.ensure_running(&actor).await {
            Ok(process) => process,
            Err(error) => {
                actor.release_voice(slot);
                return Err(error);
            }
        };
        let threads = Arc::clone(&self.inner.threads);
        let thread_id = id.to_owned();
        let fetched = blocking(move || threads.get(&thread_id).map_err(SupervisorError::from))
            .await
            .and_then(|fetched| fetched);
        let title = match fetched {
            Ok(thread) => thread.title,
            Err(error) => {
                actor.release_voice(slot);
                return Err(error);
            }
        };
        // The realtime model knows only its instructions, so they carry the thread's conversation (DMN-105).
        let instructions = actor
            .log
            .with_state(|state| crate::live_context::live_instructions(&title, state));
        self.push(&actor, ThreadEvent::LiveSession { owner: Some(owner) });
        let call = OmpCall::LiveConnect(LiveConnectCommand {
            sdp_offer,
            voice: None,
            instructions: Some(instructions),
        });
        let answer = blocking(move || process.call(call)).await?;
        actor.touch();
        match answer {
            Ok(value) => serde_json::from_value(value).map_err(|error| {
                SupervisorError::Internal(format!("unexpected live_connect answer: {error}"))
            }),
            Err(error) => {
                match &error {
                    // omp refused before a session existed: no `live_end` will close it.
                    omp_rpc::Error::Command { error, .. }
                        if LIVE_NOT_STARTED.iter().any(|known| error.contains(known)) =>
                    {
                        if actor.release_voice(slot) {
                            self.push(&actor, ThreadEvent::LiveSession { owner: None });
                        }
                    }
                    // The session started and failed: omp's `live_end` frees the slot.
                    omp_rpc::Error::Command { .. } => {}
                    // The process is gone, and the session with it.
                    _ => {
                        actor.release_voice(slot);
                    }
                }
                Err(live_failure(error))
            }
        }
    }

    /// Stops the thread's live voice session (DMN-102); a thread whose omp does not run has none, and omp succeeds
    /// without one.
    pub async fn live_stop(&self, id: &str) -> Result<()> {
        let actor = self.actor(id).await?;
        let Some(process) = actor.process() else {
            return Ok(());
        };
        match blocking(move || process.call(OmpCall::LiveStop)).await? {
            Ok(_) | Err(omp_rpc::Error::Closed) => Ok(()),
            Err(error) => Err(live_failure(error)),
        }
    }

    /// Mutes or unmutes the thread's live voice session (DMN-102); returns the mute omp applied.
    pub async fn live_mute(&self, id: &str, muted: bool) -> Result<bool> {
        let actor = self.actor(id).await?;
        let process = actor
            .process()
            .filter(|_| lock(&actor.voice).is_some())
            .ok_or(SupervisorError::NoLiveSession)?;
        let call = OmpCall::LiveMute(LiveMuteCommand { muted: Some(muted) });
        match blocking(move || process.call(call)).await? {
            Ok(value) => value.get("muted").and_then(Value::as_bool).ok_or_else(|| {
                SupervisorError::Internal(format!("unexpected live_mute answer: {value}"))
            }),
            Err(omp_rpc::Error::Command { error, .. }) if error.contains(LIVE_NO_SESSION) => {
                Err(SupervisorError::NoLiveSession)
            }
            Err(error) => Err(live_failure(error)),
        }
    }

    /// Stops every live voice session started by `connection`, which closed (DMN-103).
    pub async fn release_live(&self, connection: u64) {
        let owned: Vec<String> = lock(&self.inner.actors)
            .values()
            .filter(|actor| {
                lock(&actor.voice)
                    .as_ref()
                    .is_some_and(|slot| slot.owner.connection == connection)
            })
            .map(|actor| actor.id.clone())
            .collect();
        for id in owned {
            if let Err(error) = self.live_stop(&id).await {
                warn!(thread = %id, %error, "cannot stop the live session of a closed connection");
            }
        }
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

    /// Brings the live process's roots in line with the repositories discovered in its workspace (DMN-029):
    /// `/add-dir` for new ones, `/remove-dir` for roots that disappeared, through `prompt` (local commands). omp
    /// refuses them while the agent streams, so a busy thread syncs once its session settles. A process still starting
    /// is waited for (it holds the lifecycle lock), since its flags come from a scan made before. A stopped thread
    /// needs nothing: its next launch passes the roots.
    pub async fn sync_roots(&self, id: &str) -> Result<()> {
        let Some(actor) = self.lookup(id) else {
            return Ok(());
        };
        let _lifecycle = actor.lifecycle.lock().await;
        let Some(process) = actor.process() else {
            actor.roots_pending.store(false, Ordering::SeqCst);
            return Ok(());
        };
        if !actor.status().settled {
            actor.roots_pending.store(true, Ordering::SeqCst);
            return Ok(());
        }
        actor.roots_pending.store(false, Ordering::SeqCst);
        let threads = Arc::clone(&self.inner.threads);
        let workspaces = Arc::clone(&self.inner.workspaces);
        let thread_id = actor.id.clone();
        let (desired, recorded) = blocking(move || {
            let thread = threads.get(&thread_id)?;
            let desired = root_paths(&workspaces.discovered(&thread.workspace_id)?);
            Ok::<_, SupervisorError>((desired, threads.roots(&thread_id)?))
        })
        .await??;
        self.apply_roots(&actor, process, desired, &recorded).await
    }

    /// Sends `/add-dir` for `desired` roots not `recorded` and `/remove-dir` for recorded ones no longer desired,
    /// then records `desired`.
    async fn apply_roots(
        &self,
        actor: &Actor,
        process: Arc<dyn AgentProcess>,
        desired: Vec<PathBuf>,
        recorded: &[PathBuf],
    ) -> Result<()> {
        let added = desired
            .iter()
            .filter(|root| !recorded.contains(root))
            .map(|root| format!("/add-dir {}", root.display()));
        let removed = recorded
            .iter()
            .filter(|root| !desired.contains(root))
            .map(|root| format!("/remove-dir {}", root.display()));
        let commands: Vec<String> = added.chain(removed).collect();
        if !commands.is_empty() {
            blocking(move || {
                for message in commands {
                    process.prompt(PromptCommand {
                        message,
                        images: None,
                        streaming_behavior: None,
                    })?;
                }
                Ok::<_, omp_rpc::Error>(())
            })
            .await??;
            actor.touch();
        }
        let threads = Arc::clone(&self.inner.threads);
        let thread_id = actor.id.clone();
        blocking(move || threads.set_roots(&thread_id, &desired)).await??;
        Ok(())
    }

    /// Stops every process (DMN-005): closing the `omp-rpc` client closes omp's stdin, then terminates its process
    /// group. Gives up waiting after [`SHUTDOWN_GRACE`]. Later starts are refused.
    pub async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::SeqCst);
        lock(&self.inner.widget_host).take();
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
        // Read-only tools cannot change files: skip the `git status` of a full scan after them (DMN-023).
        let tool_end = matches!(
            &event,
            ThreadEvent::Omp {
                frame: RpcNotification::RpcAgentEvent(RpcAgentEvent::ToolExecutionEnd(end))
            } if !READ_ONLY_TOOLS.contains(&end.tool_name.as_str())
        );
        if matches!(
            &event,
            ThreadEvent::Omp {
                frame: RpcNotification::LiveEnd(_)
            }
        ) {
            // omp ended the session: the thread may start another (DMN-101).
            lock(&actor.voice).take();
        }
        let before = actor.status();
        actor.log.push(event);
        actor.touch();
        let after = actor.status();
        if after != before {
            let _ = self.inner.changes.send(actor.id.clone());
        }
        if tool_end {
            let _ = self.inner.tool_ends.send(actor.id.clone());
        }
        if after.settled && !before.settled && actor.roots_pending.load(Ordering::SeqCst) {
            let supervisor = self.clone();
            let id = actor.id.clone();
            tokio::spawn(async move {
                if let Err(error) = supervisor.sync_roots(&id).await {
                    warn!(thread = %id, %error, "root sync failed");
                }
            });
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

    /// How to start the thread's omp (DMN-030), from a fresh scan of its workspace (DMN-023); with the roots its
    /// session had and the ones it gets now.
    async fn launch_spec(
        &self,
        actor: &Actor,
    ) -> Result<(Thread, LaunchSpec, Vec<PathBuf>, Vec<PathBuf>)> {
        let id = actor.id.clone();
        let threads = Arc::clone(&self.inner.threads);
        let workspaces = Arc::clone(&self.inner.workspaces);
        let (thread, ws, repos, recorded) = blocking(move || {
            let thread = threads.get(&id)?;
            let (view, _) = workspaces.rescan(&thread.workspace_id)?;
            let recorded = threads.roots(&id)?;
            Ok::<_, SupervisorError>((thread, view.workspace, view.repos, recorded))
        })
        .await??;
        let add_dirs = root_paths(&repos);
        let data = &self.inner.data;
        let spec = LaunchSpec {
            program: self.inner.config.omp_program.clone(),
            cwd: ws.path.clone(),
            add_dirs: add_dirs.clone(),
            session_dir: thread.session_dir.clone(),
            model: thread.model.clone(),
            approval_mode: thread.approval_mode.clone(),
            append_system_prompt: Some(conventions::append_system_prompt(
                &ws.name,
                &ws.path,
                self.inner.config.agent_dir.as_deref(),
                &data.widgets(),
            )),
            extension: Some(data.omp_extension()),
            stderr_log: data.thread_log(&actor.id),
            env: self.inner.config.omp_env.clone(),
        };
        Ok((thread, spec, recorded, add_dirs))
    }

    async fn launch(&self, actor: &Arc<Actor>) -> Result<Arc<dyn AgentProcess>> {
        let (thread, spec, mut recorded, add_dirs) = self.launch_spec(actor).await?;
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

        let widget_host = lock(&self.inner.widget_host).clone();
        let tools = host_tools::thread_tools(
            self.file_opener(&actor.id),
            self.title_setter(&actor.id),
            &thread.workspace_id,
            widget_host,
        );
        let check_host = lock(&self.inner.widget_host)
            .clone()
            .map(|host| (thread.workspace_id.clone(), host));
        let uris = match widgets::host_uris(self.inner.config.builtin_widgets.clone(), check_host) {
            Ok(uris) => uris,
            Err(error) => {
                self.fail(actor, error.to_string(), String::new());
                return Err(SupervisorError::Start(error.to_string()));
            }
        };
        let launcher = Arc::clone(&self.inner.launcher);
        let launch_spec = spec.clone();
        let launched = match blocking(move || launcher.launch(&launch_spec, tools, uris)).await? {
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
        // The flags added today's roots; the session still has the ones it saved before (S-10).
        recorded.extend(add_dirs.iter().cloned());
        if let Err(error) = self
            .apply_roots(actor, Arc::clone(&process), add_dirs, &recorded)
            .await
        {
            warn!(thread = %actor.id, %error, "cannot remove stale roots");
        }
        Ok(process)
    }

    /// What the thread's `omfp_open_file` host tool calls (DMN-060): [`Self::open_file`] on this thread. It holds the
    /// supervisor weakly (the tools live as long as the process it owns).
    fn file_opener(&self, id: &str) -> host_tools::OpenFile {
        let inner = Arc::downgrade(&self.inner);
        let id = id.to_owned();
        Arc::new(move |path: &str, line: Option<u64>| {
            let inner = Weak::upgrade(&inner).ok_or("the daemon is shutting down")?;
            Self { inner }.open_file(&id, path, line)
        })
    }

    /// What the thread's `set_thread_title` host tool calls (DMN-060): stores a set title, refreshes the index and
    /// names the omp session. It runs on an `omp-rpc` handler thread, so the store call blocks there and the session
    /// rename goes to the runtime; it holds the supervisor weakly (the tools live as long as the process it owns).
    fn title_setter(&self, id: &str) -> host_tools::SetTitle {
        let inner = Arc::downgrade(&self.inner);
        let runtime = tokio::runtime::Handle::current();
        let id = id.to_owned();
        Arc::new(move |title: &str| {
            let inner = Weak::upgrade(&inner).ok_or("the daemon is shutting down")?;
            inner
                .threads
                .rename(&id, title, TitleSource::Set)
                .map_err(|error| error.to_string())?;
            let _ = inner.changes.send(id.clone());
            let supervisor = Self { inner };
            let (id, title) = (id.clone(), title.to_owned());
            runtime.spawn(async move { supervisor.name_session(&id, title).await });
            Ok(())
        })
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
            // A live session ends with its process (DMN-103).
            lock(&actor.voice).take();
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

/// A failed live command, readable (DMN-104); other errors keep their meaning (a process gone stays one).
fn live_failure(error: omp_rpc::Error) -> SupervisorError {
    match error {
        omp_rpc::Error::Command { error, .. } => SupervisorError::Live {
            message: live_error_message(&error),
            omp: error,
        },
        other => other.into(),
    }
}

/// The `--add-dir` roots for a workspace's repositories: every one but the folder itself, by its path inside the
/// workspace (DMN-029).
fn root_paths(repos: &[crate::workspaces::Discovered]) -> Vec<PathBuf> {
    repos
        .iter()
        .filter(|repo| !repo.is_root)
        .map(|repo| repo.path.clone())
        .collect()
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
