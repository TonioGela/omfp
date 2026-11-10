//! A scripted [`AgentLauncher`] for component tests (L1, L2). Each launch replays a recorded thread fixture
//! (`fixtures/threads/<omp version>/*.jsonl`, the format of TST-012), so the fake only sends frames omp really sent
//! (TST-014): the frames before the first turn at startup, then one segment per prompt and per answered interaction.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use omfp_core::ThreadEvent;
use omp_rpc::{
    Event, ExtensionUiResponse, HostTool, OpenSessionResult, PromptCommand, ReadyEvent,
    RpcAgentEvent, RpcNotification,
};
use serde_json::{Value, json};
use tokio::sync::watch;

use super::{AgentLauncher, AgentProcess, Launched, Lease, OmpCall, PromptAdmission};
use crate::omp::{LaunchSpec, StartError, Startup};
use crate::store::new_id;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What one fake process plays: its startup and the frames it sends, in segments.
#[derive(Debug, Clone, PartialEq)]
pub struct FakeRun {
    startup: Startup,
    /// Segment 0 is sent at launch; each prompt or answer sends the next one.
    segments: VecDeque<Vec<RpcNotification>>,
}

impl FakeRun {
    /// A fresh session without history or frames.
    #[must_use]
    pub fn fresh() -> Self {
        Self {
            startup: Startup {
                ready: default_ready(),
                session: OpenSessionResult {
                    cancelled: false,
                    resumed: false,
                    session_id: new_id(),
                    session_file: None,
                },
                history: Vec::new(),
            },
            segments: VecDeque::new(),
        }
    }

    /// Reads a fixture file.
    pub fn from_fixture(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_jsonl(&text).map_err(std::io::Error::other)
    }

    /// Parses fixture lines (`omfp_core::ThreadEvent` as JSON): `history` gives the startup, `process` events are the
    /// daemon's own and skipped, omp frames are split into segments at each turn start (`agent_start` after the
    /// startup or after `session_settled`) and after each `interaction_resolved`.
    pub fn from_jsonl(text: &str) -> Result<Self, String> {
        let mut run = Self::fresh();
        run.segments.push_back(Vec::new());
        let mut ready = None;
        let mut turn_open = false;
        for (number, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let event: ThreadEvent = serde_json::from_str(line)
                .map_err(|error| format!("fixture line {}: {error}", number + 1))?;
            match event {
                ThreadEvent::History { session, messages } => {
                    run.startup.session = OpenSessionResult {
                        cancelled: false,
                        resumed: session.resumed,
                        session_id: session.session_id,
                        session_file: session.session_file,
                    };
                    run.startup.history = messages;
                }
                ThreadEvent::Omp { frame } => {
                    match &frame {
                        RpcNotification::Ready(event) if ready.is_none() => {
                            ready = Some(event.clone());
                        }
                        RpcNotification::RpcAgentEvent(RpcAgentEvent::AgentStart(_))
                            if !turn_open =>
                        {
                            turn_open = true;
                            run.segments.push_back(Vec::new());
                        }
                        RpcNotification::SessionSettled(_) => turn_open = false,
                        _ => {}
                    }
                    if let Some(segment) = run.segments.back_mut() {
                        segment.push(frame);
                    }
                }
                ThreadEvent::InteractionResolved { .. } => run.segments.push_back(Vec::new()),
                ThreadEvent::Process { .. } | ThreadEvent::Unknown { .. } => {}
            }
        }
        if let Some(ready) = ready {
            run.startup.ready = ready;
        }
        Ok(run)
    }

    /// The startup the launch reports.
    #[must_use]
    pub fn startup(&self) -> &Startup {
        &self.startup
    }
}

fn default_ready() -> ReadyEvent {
    ReadyEvent {
        protocol_version: Some(2),
        supported_protocol_versions: Some(vec![1, 2]),
        max_frame_bytes: None,
        max_reassembled_frame_bytes: None,
        supports_bash_exclude_from_context: Some(true),
    }
}

/// A scripted answer to a command.
#[derive(Debug, Clone, PartialEq)]
pub enum FakeReply {
    Ok(Value),
    /// omp's `success: false`, with its `code`.
    Fail {
        message: String,
        code: Option<String>,
    },
}

/// A command a fake process received.
#[derive(Debug, Clone)]
pub enum FakeCall {
    Prompt(PromptCommand),
    Call(OmpCall),
    Respond(ExtensionUiResponse),
}

/// Holds calls of one command until opened.
#[derive(Debug, Clone)]
pub struct Gate {
    arrived: watch::Sender<usize>,
    open: Arc<(Mutex<bool>, Condvar)>,
}

impl Gate {
    fn new() -> Self {
        Self {
            arrived: watch::Sender::new(0),
            open: Arc::new((Mutex::new(false), Condvar::new())),
        }
    }

    /// Waits until `count` calls are held at the gate.
    pub async fn arrived(&self, count: usize) {
        let mut arrived = self.arrived.subscribe();
        let _ = arrived.wait_for(|arrived| *arrived >= count).await;
    }

    /// Lets every held and later call through.
    pub fn open(&self) {
        let (open, changed) = &*self.open;
        *lock(open) = true;
        changed.notify_all();
    }

    fn pass(&self) {
        self.arrived.send_modify(|arrived| *arrived += 1);
        let (open, changed) = &*self.open;
        let mut guard = lock(open);
        while !*guard {
            guard = changed.wait(guard).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[derive(Debug, Default)]
struct Script {
    runs: VecDeque<FakeRun>,
    replies: HashMap<String, FakeReply>,
    gates: HashMap<String, Gate>,
    held: HashSet<String>,
    fail_next: Option<String>,
    launches: Vec<FakeLaunch>,
}

/// The fake launcher. Without queued runs, every launch plays [`FakeRun::fresh`].
#[derive(Debug, Clone, Default)]
pub struct FakeLauncher {
    script: Arc<Mutex<Script>>,
}

/// One launch, as the fake saw it.
#[derive(Debug, Clone)]
pub struct FakeLaunch {
    pub spec: LaunchSpec,
    /// Names of the host tools registered at launch (DMN-060).
    pub tools: Vec<String>,
    pub process: FakeProcess,
}

impl FakeLauncher {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues the run the next launch plays.
    pub fn push_run(&self, run: FakeRun) {
        lock(&self.script).runs.push_back(run);
    }

    /// Answers every later `command` (omp name, e.g. `get_state`) with `reply`; default: the smallest response data
    /// of the command's wire type (see [`default_data`]).
    pub fn reply(&self, command: &str, reply: FakeReply) {
        lock(&self.script).replies.insert(command.to_owned(), reply);
    }

    /// Holds every later `command` until the returned gate opens.
    #[must_use]
    pub fn gate(&self, command: &str) -> Gate {
        lock(&self.script)
            .gates
            .entry(command.to_owned())
            .or_insert_with(Gate::new)
            .clone()
    }

    /// Reports `session_id` as owned by another omp process (DMN-033).
    pub fn hold_lease(&self, session_id: &str) {
        lock(&self.script).held.insert(session_id.to_owned());
    }

    /// Makes the next launch fail with `message`.
    pub fn fail_next_launch(&self, message: &str) {
        lock(&self.script).fail_next = Some(message.to_owned());
    }

    /// Every launch so far, oldest first.
    #[must_use]
    pub fn launches(&self) -> Vec<FakeLaunch> {
        lock(&self.script).launches.clone()
    }

    /// The process of launch `index`.
    #[must_use]
    pub fn process(&self, index: usize) -> FakeProcess {
        lock(&self.script).launches[index].process.clone()
    }
}

impl AgentLauncher for FakeLauncher {
    fn launch(&self, spec: &LaunchSpec, tools: Vec<HostTool>) -> Result<Launched, StartError> {
        let mut script = lock(&self.script);
        if let Some(message) = script.fail_next.take() {
            return Err(StartError::Spawn(std::io::Error::other(message)));
        }
        let mut run = script.runs.pop_front().unwrap_or_else(FakeRun::fresh);
        let (sender, events) = channel();
        let startup_frames = run.segments.pop_front().unwrap_or_default();
        let process = FakeProcess {
            shared: Arc::new(Shared {
                sender: Mutex::new(Some(sender)),
                segments: Mutex::new(run.segments),
                calls: Mutex::new(Vec::new()),
                session_id: run.startup.session.session_id.clone(),
                script: Arc::clone(&self.script),
            }),
        };
        process.emit(startup_frames);
        script.launches.push(FakeLaunch {
            spec: spec.clone(),
            tools: tools
                .iter()
                .map(|tool| tool.definition().name.clone())
                .collect(),
            process: process.clone(),
        });
        Ok(Launched {
            process: Arc::new(FakeAgent(process)),
            startup: run.startup,
            events,
        })
    }

    fn session_lease(&self, _spec: &LaunchSpec, session_id: &str) -> Result<Lease, StartError> {
        Ok(if lock(&self.script).held.contains(session_id) {
            Lease::Held
        } else {
            Lease::Free
        })
    }
}

#[derive(Debug)]
struct Shared {
    sender: Mutex<Option<Sender<Event>>>,
    segments: Mutex<VecDeque<Vec<RpcNotification>>>,
    calls: Mutex<Vec<FakeCall>>,
    /// The session the process reports in `get_state`.
    session_id: String,
    script: Arc<Mutex<Script>>,
}

/// Test handle on one fake process.
#[derive(Debug, Clone)]
pub struct FakeProcess {
    shared: Arc<Shared>,
}

impl FakeProcess {
    /// Every command received so far.
    #[must_use]
    pub fn calls(&self) -> Vec<FakeCall> {
        lock(&self.shared.calls).clone()
    }

    /// The process exits on its own (crash): its event stream ends.
    pub fn exit(&self) {
        lock(&self.shared.sender).take();
    }

    /// Whether the event stream ended (exit, or the daemon closed the process).
    #[must_use]
    pub fn is_closed(&self) -> bool {
        lock(&self.shared.sender).is_none()
    }

    /// Sends frames now, as omp would.
    pub fn emit(&self, frames: Vec<RpcNotification>) {
        if let Some(sender) = &*lock(&self.shared.sender) {
            for frame in frames {
                let _ = sender.send(Event::Notification(frame));
            }
        }
    }

    fn play_next(&self) {
        let next = lock(&self.shared.segments).pop_front();
        if let Some(frames) = next {
            self.emit(frames);
        }
    }

    fn next_prompt_id(&self) -> Option<String> {
        lock(&self.shared.segments)
            .iter()
            .flatten()
            .find_map(|frame| match frame {
                RpcNotification::PromptResult(result) => result.id.clone(),
                _ => None,
            })
    }

    fn record(&self, call: FakeCall) {
        lock(&self.shared.calls).push(call);
    }

    fn scripted(&self, command: &str) -> (Option<FakeReply>, Option<Gate>) {
        let script = lock(&self.shared.script);
        (
            script.replies.get(command).cloned(),
            script.gates.get(command).cloned(),
        )
    }
}

/// The [`AgentProcess`] handed to the daemon; dropping it closes the process like `omp-rpc` does.
struct FakeAgent(FakeProcess);

impl Drop for FakeAgent {
    fn drop(&mut self) {
        self.0.exit();
    }
}

impl AgentProcess for FakeAgent {
    fn prompt(&self, command: PromptCommand) -> Result<PromptAdmission, omp_rpc::Error> {
        let process = &self.0;
        let local = command.message.starts_with('/');
        process.record(FakeCall::Prompt(command));
        if let (_, Some(gate)) = process.scripted("prompt") {
            gate.pass();
        }
        if process.is_closed() {
            return Err(omp_rpc::Error::Closed);
        }
        if local {
            return Ok(PromptAdmission {
                prompt_id: format!("req_{}", new_id()),
                agent_invoked: false,
            });
        }
        let prompt_id = process
            .next_prompt_id()
            .unwrap_or_else(|| format!("req_{}", new_id()));
        process.play_next();
        Ok(PromptAdmission {
            prompt_id,
            agent_invoked: true,
        })
    }

    fn call(&self, call: OmpCall) -> Result<Value, omp_rpc::Error> {
        let process = &self.0;
        let command = call.name();
        process.record(FakeCall::Call(call));
        let (reply, gate) = process.scripted(command);
        if let Some(gate) = gate {
            gate.pass();
        }
        if process.is_closed() {
            return Err(omp_rpc::Error::Closed);
        }
        match reply
            .unwrap_or_else(|| FakeReply::Ok(default_data(command, &process.shared.session_id)))
        {
            FakeReply::Ok(value) => Ok(value),
            FakeReply::Fail { message, code } => Err(omp_rpc::Error::Command {
                command: command.to_owned(),
                error: message,
                code,
            }),
        }
    }

    fn respond(&self, response: ExtensionUiResponse) -> Result<(), omp_rpc::Error> {
        let process = &self.0;
        process.record(FakeCall::Respond(response));
        if process.is_closed() {
            return Err(omp_rpc::Error::Closed);
        }
        process.play_next();
        Ok(())
    }
}

/// omp's response data to `command` when the test scripted none: the smallest value of the command's wire type
/// (`GetAvailableModelsResult` for `get_available_models`, …), `null` for commands without data.
fn default_data(command: &str, session_id: &str) -> Value {
    match command {
        "get_state" => json!({ "sessionId": session_id }),
        "abort_and_restore_queue" => json!({ "steering": [], "followUp": [] }),
        "remove_queued_message" => json!({ "removed": false }),
        "promote_queued_message" => json!({ "promoted": false }),
        "get_available_models" => json!({ "models": [] }),
        "get_available_thinking_levels" => json!({ "levels": ["off"] }),
        "get_available_commands" => json!({ "commands": [] }),
        "compact" => json!({ "summary": "", "firstKeptEntryId": "", "tokensBefore": 0 }),
        "get_tree" => json!({ "tree": [], "leafId": null }),
        "navigate_tree" | "fork" => json!({ "cancelled": false }),
        "get_subagents" => json!({ "subagents": [] }),
        "get_subagent_messages" => json!({
            "sessionFile": "", "fromByte": 0, "nextByte": 0, "reset": false, "entries": [], "messages": []
        }),
        "cancel_subagent" => json!(false),
        "goal" => json!({ "goal": null, "state": null }),
        "btw" => json!({ "record": {
            "question": "", "answer": "", "status": "complete", "createdAt": 0, "updatedAt": 0, "id": "btw",
            "leafId": null
        }}),
        "bash" => json!({
            "output": "", "cancelled": false, "truncated": false, "totalLines": 0, "totalBytes": 0,
            "outputLines": 0, "outputBytes": 0, "exitCode": 0
        }),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> FakeRun {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/threads")
            .join(omfp_core::OMP_VERSION)
            .join(format!("{name}.jsonl"));
        FakeRun::from_fixture(&path).unwrap()
    }

    fn frame_types(frames: &[RpcNotification]) -> Vec<String> {
        frames
            .iter()
            .map(|frame| {
                serde_json::to_value(frame).unwrap()["type"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect()
    }

    // spec: TST-014
    #[test]
    fn a_fixture_splits_into_startup_turn_and_answer_segments() {
        let run = fixture("approval_granted");

        assert_eq!(run.segments.len(), 3);
        assert_eq!(frame_types(&run.segments[0])[0], "ready");
        assert_eq!(frame_types(&run.segments[1])[0], "agent_start");
        assert!(frame_types(&run.segments[1]).ends_with(&["extension_ui_request".to_owned()]));
        assert!(frame_types(&run.segments[2]).ends_with(&["session_settled".to_owned()]));
        assert_eq!(
            run.startup.ready.supports_bash_exclude_from_context,
            Some(true)
        );
    }

    // spec: TST-014
    #[test]
    fn the_fake_replays_only_recorded_frames_in_order() {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/threads")
                .join(omfp_core::OMP_VERSION)
                .join("text_reply.jsonl"),
        )
        .unwrap();
        let recorded: Vec<RpcNotification> = text
            .lines()
            .filter_map(|line| match serde_json::from_str(line).unwrap() {
                ThreadEvent::Omp { frame } => Some(frame),
                _ => None,
            })
            .collect();
        let launcher = FakeLauncher::new();
        launcher.push_run(FakeRun::from_jsonl(&text).unwrap());
        let spec = LaunchSpec {
            program: "omp".into(),
            cwd: "/home".into(),
            add_dirs: Vec::new(),
            session_dir: "/sessions".into(),
            model: None,
            approval_mode: None,
            append_system_prompt: None,
            stderr_log: "/omp.log".into(),
            env: Vec::new(),
        };

        let launched = launcher.launch(&spec, Vec::new()).unwrap();
        let admission = launched
            .process
            .prompt(PromptCommand {
                message: "say hi".into(),
                images: None,
                streaming_behavior: None,
            })
            .unwrap();
        drop(launched.process);
        let replayed: Vec<RpcNotification> = launched
            .events
            .iter()
            .map(|event| match event {
                Event::Notification(frame) => frame,
                other => panic!("unexpected {other:?}"),
            })
            .collect();

        assert_eq!(replayed, recorded);
        assert_eq!(admission.prompt_id, "req_7");
        assert!(launcher.process(0).is_closed());
    }
}
