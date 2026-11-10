//! The method table: one marker type per protocol method tying its name to its params and result types.
//!
//! Results omp defines are its wire types (`pmo_core::wire`); their schema is left open.

use schemars::schema_for;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde::ser::SerializeMap;
use serde_json::{Map, Value, json};
use std::fmt::Debug;

use crate::dto::{DiffPage, GitStatus, Repository, ThreadSummary, Workspace};
use crate::params;
use pmo_core::wire;

/// A protocol method: its wire name and the types of its params and result.
pub trait Method {
    /// Wire name, e.g. `thread.prompt`.
    const NAME: &'static str;
    type Params: Serialize + DeserializeOwned + Debug + Clone + PartialEq;
    type Result: Serialize + DeserializeOwned + Debug + Clone + PartialEq;

    /// The request payload for these params.
    fn call(params: Self::Params) -> Call;
}

/// Schema of a result: its own, or the one given after `as` for omp-defined types.
macro_rules! result_schema {
    ($result:ty) => {
        schema_for!($result)
    };
    ($result:ty, $schema:ty) => {
        schema_for!($schema)
    };
}

macro_rules! methods {
    ($(
        $(#[$doc:meta])*
        $variant:ident = $name:literal ($params:ty) -> $result:ty $(as $schema:ty)?;
    )*) => {
        $(
            $(#[$doc])*
            #[derive(Debug, Clone, Copy)]
            pub struct $variant;

            impl Method for $variant {
                const NAME: &'static str = $name;
                type Params = $params;
                type Result = $result;

                fn call(params: Self::Params) -> Call {
                    Call::$variant(params)
                }
            }
        )*

        /// A method with its params: the payload of a request.
        #[derive(Debug, Clone, PartialEq)]
        pub enum Call {
            $(
                $(#[$doc])*
                $variant($params),
            )*
        }

        /// Wire names of every method, in table order.
        pub const METHODS: &[&str] = &[$($name),*];

        impl Call {
            /// Wire name of the method.
            #[must_use]
            pub fn method(&self) -> &'static str {
                match self {
                    $(Self::$variant(_) => $name,)*
                }
            }

            /// Decodes the params of `method`; `None` for an unknown method.
            pub(crate) fn decode(method: &str, params: Value) -> Option<Result<Self, serde_json::Error>> {
                match method {
                    $($name => Some(serde_json::from_value(params).map(Self::$variant)),)*
                    _ => None,
                }
            }

            pub(crate) fn serialize_params<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
                match self {
                    $(Self::$variant(params) => map.serialize_entry("params", params),)*
                }
            }
        }

        /// JSON Schemas of every method's params and result, keyed by method name (AGT-007).
        #[must_use]
        pub fn method_schemas() -> Value {
            let mut schemas = Map::new();
            $(
                schemas.insert(
                    $name.to_owned(),
                    json!({
                        "params": schema_for!($params),
                        "result": result_schema!($result $(, $schema)?),
                    }),
                );
            )*
            Value::Object(schemas)
        }
    };
}

methods! {
    /// Authenticates the connection and negotiates the protocol version.
    SystemHello = "system.hello" (params::HelloParams) -> params::HelloResult;
    /// Starts delivery of a topic.
    Subscribe = "subscribe" (params::SubscribeParams) -> params::SubscribeResult;
    /// Stops delivery of a topic.
    Unsubscribe = "unsubscribe" (params::TopicParams) -> params::Empty;
    /// Registers a git repository.
    ReposAdd = "repos.add" (params::AddRepositoryParams) -> Repository;
    /// Unregisters a repository.
    ReposRemove = "repos.remove" (params::RepositoryRef) -> params::Empty;
    /// Lists registered repositories.
    ReposList = "repos.list" (params::Empty) -> params::RepositoryList;
    /// Creates a managed worktree.
    WorkspacesCreate = "workspaces.create" (params::CreateWorkspaceParams) -> Workspace;
    /// Creates a managed worktree on a pull request's head.
    WorkspacesFromPr = "workspaces.from_pr" (params::WorkspaceFromPrParams) -> Workspace;
    /// Records an existing directory as a workspace.
    WorkspacesAdopt = "workspaces.adopt" (params::AdoptWorkspaceParams) -> Workspace;
    /// Removes a managed worktree.
    WorkspacesRemove = "workspaces.remove" (params::RemoveWorkspaceParams) -> params::Empty;
    /// Forgets a workspace without touching its directory.
    WorkspacesForget = "workspaces.forget" (params::WorkspaceRef) -> params::Empty;
    /// Git status of a workspace.
    WorkspacesStatus = "workspaces.status" (params::WorkspaceRef) -> GitStatus;
    /// One page of a workspace's diff.
    WorkspacesDiff = "workspaces.diff" (params::WorkspaceDiffParams) -> DiffPage;
    /// Creates a thread.
    ThreadsCreate = "threads.create" (params::CreateThreadParams) -> ThreadSummary;
    /// Renames or retags a thread.
    ThreadsUpdate = "threads.update" (params::UpdateThreadParams) -> ThreadSummary;
    /// Attaches a workspace to a thread.
    ThreadsAttach = "threads.attach" (params::AttachWorkspaceParams) -> ThreadSummary;
    /// Detaches a workspace from a thread.
    ThreadsDetach = "threads.detach" (params::DetachWorkspaceParams) -> ThreadSummary;
    /// Archives a thread.
    ThreadsArchive = "threads.archive" (params::ThreadRef) -> params::Empty;
    /// Deletes a thread; its workspaces are kept.
    ThreadsDelete = "threads.delete" (params::ThreadRef) -> params::Empty;
    /// Sends a prompt (omp `prompt`).
    ThreadPrompt = "thread.prompt" (params::PromptParams) -> params::PromptResult;
    /// Steers the running turn (omp `steer`).
    ThreadSteer = "thread.steer" (params::MessageParams) -> params::Empty;
    /// Queues a follow-up (omp `follow_up`).
    ThreadFollowUp = "thread.follow_up" (params::MessageParams) -> params::Empty;
    /// Aborts and returns the queued messages (omp `abort_and_restore_queue`).
    ThreadAbort = "thread.abort" (params::ThreadRef) -> wire::AbortAndRestoreQueueResult as Value;
    /// Removes a queued message (omp `remove_queued_message`).
    ThreadQueueRemove = "thread.queue.remove" (params::QueueMessageParams) -> wire::RemoveQueuedMessageResult as Value;
    /// Promotes a queued message (omp `promote_queued_message`).
    ThreadQueuePromote = "thread.queue.promote" (params::QueueMessageParams) -> wire::PromoteQueuedMessageResult as Value;
    /// Changes the model (omp `set_model`).
    ThreadSetModel = "thread.set_model" (params::SetModelParams) -> params::Empty;
    /// Changes the thinking level (omp `set_thinking_level`).
    ThreadSetThinking = "thread.set_thinking" (params::SetThinkingParams) -> params::Empty;
    /// Available models (omp `get_available_models`).
    ThreadModels = "thread.models" (params::ThreadRef) -> wire::GetAvailableModelsResult as Value;
    /// Available thinking levels (omp `get_available_thinking_levels`).
    ThreadThinkingLevels = "thread.thinking_levels" (params::ThreadRef) -> wire::GetAvailableThinkingLevelsResult as Value;
    /// Available slash commands (omp `get_available_commands`).
    ThreadCommands = "thread.commands" (params::ThreadRef) -> wire::GetAvailableCommandsResult as Value;
    /// Compacts the context (omp `compact`).
    ThreadCompact = "thread.compact" (params::CompactParams) -> wire::CompactionResult as Value;
    /// Session tree (omp `get_tree`).
    ThreadTree = "thread.tree" (params::ThreadRef) -> wire::SessionTree as Value;
    /// Moves the session leaf (omp `navigate_tree`).
    ThreadNavigate = "thread.navigate" (params::NavigateParams) -> wire::NavigateTreeResult as Value;
    /// Forks into a new session file (omp `fork`).
    ThreadFork = "thread.fork" (params::ForkParams) -> wire::CancellationResult as Value;
    /// Labels a session entry (omp `set_label`).
    ThreadLabel = "thread.label" (params::LabelParams) -> params::Empty;
    /// Subagents of the thread (omp `get_subagents`).
    ThreadSubagents = "thread.subagents" (params::ThreadRef) -> wire::GetSubagentsResult as Value;
    /// A subagent's transcript (omp `get_subagent_messages`).
    ThreadSubagentMessages = "thread.subagent_messages" (params::SubagentMessagesParams) -> wire::SubagentMessages as Value;
    /// Steers a subagent (omp `steer_subagent`).
    ThreadSubagentSteer = "thread.subagent_steer" (params::SubagentSteerParams) -> params::Empty;
    /// Cancels a subagent (omp `cancel_subagent`).
    ThreadSubagentCancel = "thread.subagent_cancel" (params::SubagentRef) -> params::Empty;
    /// Reads or changes the session goal (omp `goal`).
    ThreadGoal = "thread.goal" (params::GoalParams) -> wire::GoalResult as Value;
    /// Asks a side question (omp `btw`).
    ThreadBtw = "thread.btw" (params::BtwParams) -> wire::BtwResult as Value;
    /// Runs a shell command in the session (omp `bash`).
    ThreadBash = "thread.bash" (params::BashParams) -> wire::BashResult as Value;
    /// Git status of every workspace of the thread.
    ThreadGit = "thread.git" (params::ThreadRef) -> params::ThreadGit;
    /// Session state (omp `get_state`).
    ThreadState = "thread.state" (params::ThreadRef) -> wire::SessionState as Value;
    /// Restarts the omp process.
    ThreadRestart = "thread.restart" (params::ThreadRef) -> params::Empty;
    /// Stops the omp process.
    ThreadStop = "thread.stop" (params::ThreadRef) -> params::Empty;
    /// Answers an interactive request; `already_resolved` if another client answered first.
    InteractionRespond = "interaction.respond" (params::RespondParams) -> params::Empty;
    /// Opens a terminal.
    TerminalOpen = "terminal.open" (params::OpenTerminalParams) -> params::OpenTerminalResult;
    /// Resizes a terminal.
    TerminalResize = "terminal.resize" (params::ResizeTerminalParams) -> params::Empty;
    /// Closes a terminal.
    TerminalClose = "terminal.close" (params::TerminalRef) -> params::Empty;
    /// Lists open terminals.
    TerminalList = "terminal.list" (params::ListTerminalsParams) -> params::TerminalList;
}
