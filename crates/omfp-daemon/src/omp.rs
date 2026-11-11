//! The omp process of one thread: launch (DMN-030), session setup (DMN-031) and history load (DMN-032).
//!
//! Everything here is blocking (`omp-rpc` is a blocking client); the agent supervisor calls it from blocking tasks.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

use omfp_core::{ProcessState, SessionInfo, ThreadEvent};
use omp_rpc::{
    AgentMessage, Client, ClientOptions, Event, GetMessagesCommand, GetMessagesPageCommand,
    HostTool, HostUri, OpenSessionCommand, OpenSessionResult, ReadyEvent, SetAskDialogCommand,
    SetEventFilterResult, SetSubagentSubscriptionCommand, SubagentSubscriptionLevel,
};
use serde::Serialize;
use serde_json::Value;

/// How to start omp for a thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    /// omp executable (DMN-006).
    pub program: PathBuf,
    /// Working directory: the thread's workspace folder (DMN-030).
    pub cwd: PathBuf,
    /// The workspace's repositories, passed as `--add-dir` in order (DMN-029).
    pub add_dirs: Vec<PathBuf>,
    /// Directory holding the thread's omp sessions (ARC-020).
    pub session_dir: PathBuf,
    /// `provider/model` selector.
    pub model: Option<String>,
    /// omp `--approval-mode` value.
    pub approval_mode: Option<String>,
    /// Text passed as `--append-system-prompt`: the daemon's conventions and the user's append prompt (DMN-030).
    pub append_system_prompt: Option<String>,
    /// The daemon's omp extension root, passed as `--extension`: it ships the `widget-builder` agent (DMN-107).
    pub extension: Option<PathBuf>,
    /// File receiving omp's stderr (DMN-036).
    pub stderr_log: PathBuf,
    /// Environment overrides applied last: `Some` sets, `None` removes.
    pub env: Vec<(OsString, Option<OsString>)>,
}

impl LaunchSpec {
    /// Command-line arguments, in order.
    #[must_use]
    pub fn args(&self) -> Vec<OsString> {
        // `--session-dir … --continue` binds the session at launch, so `--add-dir` roots apply to it; a later
        // `open_session` on the same directory is a no-op that reports whether the session was resumed.
        let mut args: Vec<OsString> = vec![
            "--mode".into(),
            "rpc-ui".into(),
            "--session-dir".into(),
            self.session_dir.clone().into_os_string(),
            "--continue".into(),
        ];
        for dir in &self.add_dirs {
            args.push("--add-dir".into());
            args.push(dir.clone().into_os_string());
        }
        if let Some(model) = &self.model {
            args.push("--model".into());
            args.push(model.into());
        }
        if let Some(mode) = &self.approval_mode {
            args.push("--approval-mode".into());
            args.push(mode.into());
        }
        if let Some(text) = &self.append_system_prompt {
            args.push("--append-system-prompt".into());
            args.push(text.into());
        }
        if let Some(dir) = &self.extension {
            args.push("--extension".into());
            args.push(dir.clone().into_os_string());
        }
        args
    }

    /// The process to spawn; stdin/stdout are set by `omp-rpc`.
    pub fn command(&self) -> std::io::Result<Command> {
        if let Some(parent) = self.stderr_log.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::create_dir_all(&self.cwd)?;
        std::fs::create_dir_all(&self.session_dir)?;
        let stderr = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.stderr_log)?;
        let mut command = Command::new(&self.program);
        command
            .args(self.args())
            .current_dir(&self.cwd)
            .stderr(Stdio::from(stderr));
        for (key, value) in &self.env {
            match value {
                Some(value) => command.env(key, value),
                None => command.env_remove(key),
            };
        }
        Ok(command)
    }
}

/// Why omp could not be started or prepared.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("cannot spawn omp: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("omp did not become ready: {0}")]
    Connect(omp_rpc::Error),
    #[error("omp does not support RPC protocol v2 (supported: {0:?})")]
    Protocol(Vec<i64>),
    #[error("omp session setup failed at `{step}`: {error}")]
    Setup {
        step: &'static str,
        error: omp_rpc::Error,
    },
    #[error("omp refused to open the session directory")]
    OpenCancelled,
    #[error("cannot check the omp session lease: {0}")]
    LeaseCheck(String),
}

/// What session setup found.
#[derive(Debug, Clone, PartialEq)]
pub struct Startup {
    pub ready: ReadyEvent,
    pub session: OpenSessionResult,
    /// Transcript of the resumed session (empty for a fresh one).
    pub history: Vec<AgentMessage>,
}

/// `set_event_filter` with delta message updates. The generated `SetEventFilterCommand` of omp-rpc 18.18.6 lacks
/// `messageUpdates`, so the daemon sends its own frame; the response type is the generated one.
#[derive(Debug, Serialize)]
struct SetEventFilterDelta {
    events: Option<Vec<String>>,
    #[serde(rename = "messageUpdates")]
    message_updates: &'static str,
}

impl omp_rpc::Command for SetEventFilterDelta {
    const NAME: &'static str = "set_event_filter";
    const TIMEOUT_MS: Option<u64> = None;
    type Output = SetEventFilterResult;

    fn decode(data: Option<Value>) -> Result<Self::Output, serde_json::Error> {
        serde_json::from_value(data.unwrap_or_else(|| Value::Object(serde_json::Map::new())))
    }
}

/// A running omp process connected over RPC.
pub struct OmpProcess {
    client: Arc<Client>,
    events: Option<Receiver<Event>>,
    next_request: u64,
}

impl std::fmt::Debug for OmpProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OmpProcess")
            .field("pid", &self.client.pid())
            .finish_non_exhaustive()
    }
}

/// Messages per `get_messages_page` call while loading history.
const HISTORY_PAGE: i64 = 200;

impl OmpProcess {
    /// Spawns omp with the host tools (DMN-060) and host URI schemes (DMN-062), waits for `ready`, and runs the
    /// session setup sequence (DMN-031, DMN-032).
    pub fn start(
        spec: &LaunchSpec,
        tools: Vec<HostTool>,
        uris: Vec<HostUri>,
    ) -> Result<(Self, Startup), StartError> {
        // `omp-rpc` numbers requests `req_1`, `req_2`, … and does not expose the numbers; the daemon needs the id of
        // each prompt (the `id` of its `prompt_result`, PRT-030), so it counts every request made on the client:
        // the protocol negotiation, the host tool registration and the host URI registration happen at connect.
        let mut requests: u64 = 1 + u64::from(!tools.is_empty()) + u64::from(!uris.is_empty());
        let options = ClientOptions {
            tools,
            uris,
            ..ClientOptions::default()
        };
        let (client, events) =
            Client::spawn(spec.command()?, options).map_err(StartError::Connect)?;
        let ready = client.ready().clone();
        let supported = ready
            .supported_protocol_versions
            .clone()
            .unwrap_or_default();
        if !supported.contains(&2) || client.protocol_version() != 2 {
            return Err(StartError::Protocol(supported));
        }

        let setup = |step: &'static str| move |error| StartError::Setup { step, error };
        let session = client
            .call(&OpenSessionCommand {
                session_dir: spec.session_dir.to_string_lossy().into_owned(),
                provider: None,
                model_id: None,
            })
            .map_err(setup("open_session"))?;
        if session.cancelled {
            return Err(StartError::OpenCancelled);
        }
        client
            .call(&SetEventFilterDelta {
                events: None,
                message_updates: "delta",
            })
            .map_err(setup("set_event_filter"))?;
        client
            .call(&SetSubagentSubscriptionCommand {
                level: SubagentSubscriptionLevel::Progress,
            })
            .map_err(setup("set_subagent_subscription"))?;
        client
            .call(&SetAskDialogCommand { enabled: true })
            .map_err(setup("set_ask_dialog"))?;
        requests += 4;
        let history = load_history(&client, &mut requests).map_err(setup("get_messages"))?;

        Ok((
            Self {
                client: Arc::new(client),
                events: Some(events),
                next_request: requests + 1,
            },
            Startup {
                ready,
                session,
                history,
            },
        ))
    }

    /// Shared handle for commands; `omp-rpc` allows concurrent calls.
    #[must_use]
    pub fn client(&self) -> &Arc<Client> {
        &self.client
    }

    /// Frames that are not command responses; can be taken once.
    pub fn take_events(&mut self) -> Option<Receiver<Event>> {
        self.events.take()
    }

    /// The number `omp-rpc` gives the next request on this client (`req_<n>`), as long as every request so far went
    /// through [`OmpProcess::start`].
    #[must_use]
    pub fn next_request(&self) -> u64 {
        self.next_request
    }

    /// The shared client and, unless already taken, the event receiver.
    #[must_use]
    pub fn into_parts(self) -> (Arc<Client>, Option<Receiver<Event>>) {
        (self.client, self.events)
    }
}

/// The session transcript, paged like `Client::get_messages` (protocol v2), counting the requests made: a page
/// request that finds the session busy or the cursor stale falls back to one `get_messages`.
fn load_history(client: &Client, requests: &mut u64) -> Result<Vec<AgentMessage>, omp_rpc::Error> {
    let mut messages = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        *requests += 1;
        let page = match client.call(&GetMessagesPageCommand {
            cursor: cursor.clone(),
            limit: Some(HISTORY_PAGE),
        }) {
            Ok(page) => page,
            Err(omp_rpc::Error::Command { code, .. })
                if matches!(code.as_deref(), Some("session_busy" | "stale_cursor")) =>
            {
                *requests += 1;
                return client.call(&GetMessagesCommand {});
            }
            Err(error) => return Err(error),
        };
        messages.extend(page.messages);
        match page.next_cursor {
            Some(next) if Some(&next) != cursor.as_ref() => cursor = Some(next),
            _ => break,
        }
    }
    Ok(messages)
}

/// The reducer input for a frame from the event receiver (DMN-040, DMN-042).
#[must_use]
pub fn thread_event(event: Event) -> ThreadEvent {
    match event {
        Event::Notification(frame) => ThreadEvent::Omp { frame },
        Event::UnmatchedResponse(response) => ThreadEvent::Unknown {
            raw: serde_json::to_value(response).unwrap_or(Value::Null),
        },
        Event::Undecodable { raw, .. } => ThreadEvent::Unknown { raw },
    }
}

/// Reducer inputs announcing a started process: its history (DMN-032), then `Running`.
#[must_use]
pub fn startup_events(startup: &Startup) -> Vec<ThreadEvent> {
    vec![
        ThreadEvent::History {
            session: SessionInfo {
                session_id: startup.session.session_id.clone(),
                session_file: startup.session.session_file.clone(),
                resumed: startup.session.resumed,
            },
            messages: startup.history.clone(),
        },
        ThreadEvent::Process {
            state: ProcessState::Running,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> LaunchSpec {
        LaunchSpec {
            program: "omp".into(),
            cwd: "/ws/checkout".into(),
            add_dirs: vec![
                "/ws/checkout/frontend".into(),
                "/ws/checkout/backend".into(),
            ],
            session_dir: "/data/threads/t1/sessions".into(),
            model: Some("anthropic/claude".into()),
            approval_mode: Some("always-ask".into()),
            append_system_prompt: Some("line one\nline two".into()),
            extension: Some("/data/omp".into()),
            stderr_log: "/data/threads/t1/omp.log".into(),
            env: vec![],
        }
    }

    // spec: DMN-030, DMN-107
    #[test]
    fn args_run_rpc_ui_with_one_add_dir_per_repository_in_order() {
        let args: Vec<String> = spec()
            .args()
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "--mode",
                "rpc-ui",
                "--session-dir",
                "/data/threads/t1/sessions",
                "--continue",
                "--add-dir",
                "/ws/checkout/frontend",
                "--add-dir",
                "/ws/checkout/backend",
                "--model",
                "anthropic/claude",
                "--approval-mode",
                "always-ask",
                "--append-system-prompt",
                "line one\nline two",
                "--extension",
                "/data/omp"
            ]
        );
    }

    // spec: DMN-030
    #[test]
    fn args_omit_unset_model_approval_mode_append_prompt_and_extension() {
        let mut spec = spec();
        spec.add_dirs.clear();
        spec.model = None;
        spec.approval_mode = None;
        spec.append_system_prompt = None;
        spec.extension = None;
        let args: Vec<String> = spec
            .args()
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "--mode",
                "rpc-ui",
                "--session-dir",
                "/data/threads/t1/sessions",
                "--continue"
            ]
        );
    }

    // spec: DMN-030, DMN-036
    #[test]
    fn command_runs_in_the_workspace_folder_and_creates_its_directories() {
        let root = tempfile::tempdir().unwrap();
        let mut spec = spec();
        spec.cwd = root.path().join("workspace");
        spec.session_dir = root.path().join("sessions");
        spec.stderr_log = root.path().join("logs/omp.log");
        spec.env = vec![("KEEP".into(), Some("1".into())), ("DROP".into(), None)];

        let command = spec.command().unwrap();

        assert_eq!(command.get_current_dir(), Some(spec.cwd.as_path()));
        assert!(spec.cwd.is_dir() && spec.session_dir.is_dir() && spec.stderr_log.is_file());
        let envs: Vec<_> = command.get_envs().collect();
        assert!(envs.contains(&("KEEP".as_ref(), Some("1".as_ref()))));
        assert!(envs.contains(&("DROP".as_ref(), None)));
    }
}
