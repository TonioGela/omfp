//! Inputs of the thread reducer: everything that changes a thread's visible state, in the order the daemon applies
//! them (ARC-006, DMN-040). The same sequence is replayed by clients and recorded as fixtures (TST-012).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::wire::{AgentMessage, RpcNotification};

/// Lifecycle of the omp process behind a thread (02-daemon, "Process states"). Whether the agent is working is not
/// part of it: that follows from omp's own events (see the reducer).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ProcessState {
    /// No process; the next command that needs omp starts one.
    #[default]
    Stopped,
    /// Spawned, waiting for `ready` and session setup.
    Starting,
    /// Connected and set up.
    Running,
    /// Exited unexpectedly or failed to start (DMN-035).
    Failed {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        /// Last lines of omp's stderr.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        stderr_tail: String,
    },
}

/// The omp session a thread is bound to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_file: Option<String>,
    /// `true` when an existing session was resumed rather than created.
    pub resumed: bool,
}

/// Who answered an interactive request (DMN-051).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum Resolution {
    /// A connected client answered; `client` is its `system.hello` name.
    Client { client: String },
}

/// Who started a live voice session (DMN-100): the daemon's number of the connection (`system.hello` reports it to
/// the client, PRT-051) and that connection's `system.hello` client name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveOwner {
    pub client: String,
    pub connection: u64,
}

/// A file the agent asked to open with the `omfp_open_file` host tool (DMN-060): the window on connection
/// `connection` (the `system.hello` number, PRT-051), the one the user last used on the thread, opens `path` (as the
/// agent wrote it) at `line`, as a click on the path does. `id` tells one request from the next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenFileRequest {
    pub id: String,
    pub connection: u64,
    pub path: String,
    pub line: Option<u64>,
}

/// A notification the agent sends the user with the `omfp_notify` host tool (DMN-125): the window on connection
/// `connection`, the one the user last used on the thread, shows `title` and `body` as a notice about the thread
/// (UI-215). `id` tells one request from the next.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotifyRequest {
    pub id: String,
    pub connection: u64,
    pub title: String,
    pub body: String,
}

/// One input of the thread reducer.
#[allow(clippy::large_enum_variant)] // Mirrors omp-rpc's unboxed frame unions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThreadEvent {
    /// A frame from omp that is not a command response: session events, `prompt_result`, `session_settled`,
    /// `extension_ui_request`, `available_commands_update`, subagent frames, … (decoded by `omp-rpc`).
    Omp { frame: RpcNotification },
    /// Transcript of the session right after the process started (DMN-032). Replaces the transcript.
    History {
        session: SessionInfo,
        messages: Vec<AgentMessage>,
    },
    /// Process lifecycle change.
    Process { state: ProcessState },
    /// The daemon forwarded an answer to the interactive request with omp id `id` (DMN-051).
    InteractionResolved { id: String, resolution: Resolution },
    /// The daemon opened a live voice session for `owner` before forwarding `live_connect` (DMN-100), or closed one
    /// omp never started (`None`, DMN-101). omp's `live_*` frames arrive as [`ThreadEvent::Omp`].
    LiveSession { owner: Option<LiveOwner> },
    /// The daemon asks one window to open a file for the agent (DMN-060).
    OpenFile(OpenFileRequest),
    /// The daemon asks one window to notify the user for the agent (DMN-125).
    Notify(NotifyRequest),
    /// A frame `omp-rpc` could not decode, or a response nobody waited for (DMN-042).
    Unknown { raw: Value },
}
