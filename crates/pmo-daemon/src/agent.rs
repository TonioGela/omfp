//! The port between the agent supervisor and omp (ARC-012): launching a thread's process and forwarding commands.
//! [`OmpLauncher`] is the real implementation over [`crate::omp`]; tests use a scripted fake.

use std::sync::Arc;
use std::sync::mpsc::Receiver;

use omp_rpc::{
    AbortAndRestoreQueueCommand, BashCommand, BtwCommand, CancelSubagentCommand, Client,
    CompactCommand, Event, ExtensionUiResponse, FollowUpCommand, ForkCommand,
    GetAvailableCommandsCommand, GetAvailableModelsCommand, GetAvailableThinkingLevelsCommand,
    GetStateCommand, GetSubagentMessagesCommand, GetSubagentsCommand, GetTreeCommand, GoalCommand,
    HostTool, NavigateTreeCommand, PromoteQueuedMessageCommand, PromptCommand,
    RemoveQueuedMessageCommand, RpcInbound, SetLabelCommand, SetModelCommand,
    SetThinkingLevelCommand, SteerCommand, SteerSubagentCommand,
};
use serde::Serialize;
use serde_json::Value;

use crate::omp::{LaunchSpec, OmpProcess, StartError, Startup};

/// One omp command the daemon forwards (02-daemon, "Commands towards omp").
#[derive(Debug, Clone)]
pub enum OmpCall {
    Prompt(PromptCommand),
    Steer(SteerCommand),
    FollowUp(FollowUpCommand),
    AbortAndRestoreQueue,
    RemoveQueued(RemoveQueuedMessageCommand),
    PromoteQueued(PromoteQueuedMessageCommand),
    SetModel(SetModelCommand),
    SetThinkingLevel(SetThinkingLevelCommand),
    AvailableModels,
    ThinkingLevels,
    Commands,
    Compact(CompactCommand),
    Tree,
    NavigateTree(NavigateTreeCommand),
    Fork(ForkCommand),
    SetLabel(SetLabelCommand),
    Subagents,
    SubagentMessages(GetSubagentMessagesCommand),
    SteerSubagent(SteerSubagentCommand),
    CancelSubagent(CancelSubagentCommand),
    Goal(GoalCommand),
    Btw(BtwCommand),
    Bash(BashCommand),
    State,
}

impl OmpCall {
    /// omp's command name, for errors and logs.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Prompt(_) => "prompt",
            Self::Steer(_) => "steer",
            Self::FollowUp(_) => "follow_up",
            Self::AbortAndRestoreQueue => "abort_and_restore_queue",
            Self::RemoveQueued(_) => "remove_queued_message",
            Self::PromoteQueued(_) => "promote_queued_message",
            Self::SetModel(_) => "set_model",
            Self::SetThinkingLevel(_) => "set_thinking_level",
            Self::AvailableModels => "get_available_models",
            Self::ThinkingLevels => "get_available_thinking_levels",
            Self::Commands => "get_available_commands",
            Self::Compact(_) => "compact",
            Self::Tree => "get_tree",
            Self::NavigateTree(_) => "navigate_tree",
            Self::Fork(_) => "fork",
            Self::SetLabel(_) => "set_label",
            Self::Subagents => "get_subagents",
            Self::SubagentMessages(_) => "get_subagent_messages",
            Self::SteerSubagent(_) => "steer_subagent",
            Self::CancelSubagent(_) => "cancel_subagent",
            Self::Goal(_) => "goal",
            Self::Btw(_) => "btw",
            Self::Bash(_) => "bash",
            Self::State => "get_state",
        }
    }
}

/// A running omp process, as seen by the supervisor.
pub trait AgentProcess: Send + Sync {
    /// Sends `call` and returns omp's response data as JSON.
    fn call(&self, call: OmpCall) -> Result<Value, omp_rpc::Error>;
    /// Answers an interactive request (DMN-051).
    fn respond(&self, response: ExtensionUiResponse) -> Result<(), omp_rpc::Error>;
}

/// A freshly started process: its handle, setup results and event stream.
pub struct Launched {
    pub process: Arc<dyn AgentProcess>,
    pub startup: Startup,
    pub events: Receiver<Event>,
}

impl std::fmt::Debug for Launched {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Launched")
            .field("startup", &self.startup)
            .finish_non_exhaustive()
    }
}

/// Starts omp processes for threads.
pub trait AgentLauncher: Send + Sync {
    fn launch(&self, spec: &LaunchSpec) -> Result<Launched, StartError>;
}

/// The real launcher: `omp --mode rpc-ui` through `omp-rpc`, with the daemon's host tools.
#[derive(Debug, Clone, Default)]
pub struct OmpLauncher {
    pub tools: Vec<HostTool>,
}

impl AgentLauncher for OmpLauncher {
    fn launch(&self, spec: &LaunchSpec) -> Result<Launched, StartError> {
        let (process, startup) = OmpProcess::start(spec, self.tools.clone())?;
        let (client, events) = process.into_parts();
        let events = events.expect("a fresh process still owns its event receiver");
        Ok(Launched {
            process: Arc::new(OmpAgent(client)),
            startup,
            events,
        })
    }
}

/// [`AgentProcess`] over a connected `omp-rpc` client; dropping the last handle closes omp.
struct OmpAgent(Arc<Client>);

impl OmpAgent {
    fn run<C>(&self, command: &C) -> Result<Value, omp_rpc::Error>
    where
        C: omp_rpc::Command,
        C::Output: Serialize,
    {
        let output = self.0.call(command)?;
        serde_json::to_value(output).map_err(omp_rpc::Error::Json)
    }
}

impl AgentProcess for OmpAgent {
    fn call(&self, call: OmpCall) -> Result<Value, omp_rpc::Error> {
        match &call {
            OmpCall::Prompt(command) => self.run(command),
            OmpCall::Steer(command) => self.run(command),
            OmpCall::FollowUp(command) => self.run(command),
            OmpCall::AbortAndRestoreQueue => self.run(&AbortAndRestoreQueueCommand {}),
            OmpCall::RemoveQueued(command) => self.run(command),
            OmpCall::PromoteQueued(command) => self.run(command),
            OmpCall::SetModel(command) => self.run(command),
            OmpCall::SetThinkingLevel(command) => self.run(command),
            OmpCall::AvailableModels => self.run(&GetAvailableModelsCommand {}),
            OmpCall::ThinkingLevels => self.run(&GetAvailableThinkingLevelsCommand {}),
            OmpCall::Commands => self.run(&GetAvailableCommandsCommand {}),
            OmpCall::Compact(command) => self.run(command),
            OmpCall::Tree => self.run(&GetTreeCommand {}),
            OmpCall::NavigateTree(command) => self.run(command),
            OmpCall::Fork(command) => self.run(command),
            OmpCall::SetLabel(command) => self.run(command),
            OmpCall::Subagents => self.run(&GetSubagentsCommand {}),
            OmpCall::SubagentMessages(command) => self.run(command),
            OmpCall::SteerSubagent(command) => self.run(command),
            OmpCall::CancelSubagent(command) => self.run(command),
            OmpCall::Goal(command) => self.run(command),
            OmpCall::Btw(command) => self.run(command),
            OmpCall::Bash(command) => self.run(command),
            OmpCall::State => self.run(&GetStateCommand {}),
        }
    }

    fn respond(&self, response: ExtensionUiResponse) -> Result<(), omp_rpc::Error> {
        self.0.send(&RpcInbound::ExtensionUiResponse(response))
    }
}
