//! Params and results of the protocol methods that omfp defines itself. Results that omp defines are its wire types
//! (see `crate::method`).

use omfp_core::wire::{
    AskAnswer, GoalOp, ImageContent, LitTrue, QueuedMessageQueue, ThinkingLevel,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dto::{KeymapEntry, RepoStatus, Repository, TerminalInfo, TerminalKind, UiSettings};
use crate::topic::Topic;

/// No params, or the `{}` result.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Empty {}

/// `system.hello`: first frame of every connection (PRT-001).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelloParams {
    /// The daemon's access token (PRT-002).
    pub token: String,
    /// Protocol version the client speaks (PRT-004).
    pub protocol: u32,
    /// Client name, recorded as provenance (PRT-033), e.g. `omfp/web`.
    pub client: String,
}

/// `system.hello` result (PRT-005).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelloResult {
    pub protocol: u32,
    /// Daemon version.
    pub daemon: String,
    /// omp version.
    pub omp: String,
    pub warnings: Vec<String>,
}

/// A position in a topic's event stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub epoch: String,
    pub seq: u64,
}

/// `subscribe` params (PRT-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeParams {
    pub topic: Topic,
    /// Last event the client has; the daemon replays from there when it still can.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<Position>,
}

/// `subscribe` result: catch-up from the ring buffer, or a full state (PRT-020, PRT-021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SubscribeResult {
    /// Events from `fromSeq` follow as `event` frames.
    Replay {
        epoch: String,
        #[serde(rename = "fromSeq")]
        from_seq: u64,
    },
    /// The topic's state at `seq`; live events after `seq` follow. `state` is a `omfp_core::ThreadState` for
    /// `thread/<id>`, an `IndexSnapshot` for `index`, and a list of `RepoStatus` for `workspace/<id>`.
    Snapshot {
        epoch: String,
        seq: u64,
        state: Value,
    },
}

/// Params naming one topic (`unsubscribe`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TopicParams {
    pub topic: Topic,
}

/// `repos.add` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddRepositoryParams {
    pub path: String,
}

/// Params naming one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryRef {
    pub repository_id: String,
}

/// `repos.list` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryList {
    pub repositories: Vec<Repository>,
}

/// `workspaces.create` params (DMN-020).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceParams {
    /// Unique name; default: the folder's name for an existing folder, else `workspace` (made unique).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// An existing folder to record (not owned by the daemon); absent: a new folder under the workspaces root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// `workspaces.update` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWorkspaceParams {
    pub workspace_id: String,
    /// New unique name; the folder is not moved.
    pub name: String,
}

/// `workspaces.remove` params (DMN-028).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveWorkspaceParams {
    pub workspace_id: String,
    /// Also delete an owned folder, after removing its repositories as `workspaces.remove_repo` does.
    #[serde(default)]
    pub delete_folder: bool,
    /// Remove worktrees and clones even with uncommitted changes.
    #[serde(default)]
    pub force: bool,
}

/// Params naming one workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRef {
    pub workspace_id: String,
}

/// `workspaces.add_repo` params (DMN-024).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddRepoParams {
    pub workspace_id: String,
    /// Entry name in the folder; default: derived from the source (made unique).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub source: RepoSource,
}

/// Where a repository added to a workspace comes from (DMN-024).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepoSource {
    /// A symlink `<name> → <path>`.
    Link { path: String },
    /// `git worktree add` of a repository.
    #[serde(rename_all = "camelCase")]
    Worktree {
        /// Known repository id, or a path inside a git repository.
        repo: String,
        /// Existing branch to check out, or new branch name; default: a new `omfp/<slug>-<n>` branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Base of a new branch; default: the known repository's base ref, else its checked-out branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base_ref: Option<String>,
    },
    /// `git clone <url> <name>`.
    Clone { url: String },
    /// A pull request's head as a worktree (after S-08).
    Pr {
        /// PR URL or `owner/repo#N`.
        pr: String,
    },
}

/// `workspaces.remove_repo` params (DMN-025).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveRepoParams {
    pub workspace_id: String,
    /// The repository's name in the workspace.
    pub name: String,
    /// Remove a worktree or clone even with uncommitted changes.
    #[serde(default)]
    pub force: bool,
    /// Also delete a worktree's branch.
    #[serde(default)]
    pub delete_branch: bool,
}

/// `workspaces.status` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepoRef {
    pub workspace_id: String,
    /// The repository's name in the workspace.
    pub repo: String,
}

/// `workspaces.diff` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDiffParams {
    pub workspace_id: String,
    /// The repository's name in the workspace.
    pub repo: String,
    /// Only this file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// `nextCursor` of the previous page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// The workspace of a new thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ThreadWorkspace {
    /// An existing workspace.
    Id(String),
    /// A new workspace, as `workspaces.create` makes it; its name defaults to the thread's title, else to the first
    /// words of its first message.
    New(CreateWorkspaceParams),
}

impl Default for ThreadWorkspace {
    fn default() -> Self {
        Self::New(CreateWorkspaceParams::default())
    }
}

/// `threads.create` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateThreadParams {
    /// For scripts and agents; without it the title is generated from the conversation (DMN-073).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `{ "id": … }` or `{ "new": { "name"?, "path"? } }`; default: a new folder named after the first message.
    #[serde(default)]
    pub workspace: ThreadWorkspace,
    #[serde(default)]
    pub tags: Vec<String>,
    /// `provider/id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Value>")]
    pub thinking_level: Option<ThinkingLevel>,
    /// omp's `--approval-mode`; default `yolo` (DMN-074).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<String>,
    /// First message, sent once the thread is created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

/// `threads.update` params; absent fields are unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateThreadParams {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    /// omp's `--approval-mode`, applied from the next start of the thread's omp (DMN-074).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<String>,
}

/// Params naming one thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRef {
    pub thread_id: String,
}

/// How a prompt sent while the agent is busy is queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PromptBehavior {
    Steer,
    FollowUp,
}

/// `thread.prompt` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptParams {
    pub thread_id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(with = "Vec<Value>")]
    pub images: Vec<ImageContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behavior: Option<PromptBehavior>,
}

/// `thread.prompt` result, sent once omp admits the prompt (PRT-030).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PromptResult {
    /// The `id` of the `prompt_result` that ends this prompt.
    pub prompt_id: String,
    /// `false` when omp handled the prompt locally: no `prompt_result` follows.
    pub agent_invoked: bool,
}

/// `thread.steer` / `thread.follow_up` params.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MessageParams {
    pub thread_id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(with = "Vec<Value>")]
    pub images: Vec<ImageContent>,
}

/// `thread.queue.remove` / `thread.queue.promote` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QueueMessageParams {
    pub thread_id: String,
    /// Text of the queued message.
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<Value>")]
    pub queue: Option<QueuedMessageQueue>,
}

/// `thread.set_model` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetModelParams {
    pub thread_id: String,
    pub provider: String,
    pub model_id: String,
}

/// `thread.set_thinking` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetThinkingParams {
    pub thread_id: String,
    #[schemars(with = "Value")]
    pub level: ThinkingLevel,
}

/// `thread.compact` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompactParams {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// `thread.navigate` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NavigateParams {
    pub thread_id: String,
    pub entry_id: String,
    /// Summarize the branch being left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summarize: Option<bool>,
    /// Instructions for the summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// `thread.fork` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ForkParams {
    pub thread_id: String,
    /// Fork history through this entry; the whole session when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<String>,
}

/// `thread.label` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LabelParams {
    pub thread_id: String,
    pub entry_id: String,
    /// The label is cleared when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// `thread.subagent_messages` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubagentMessagesParams {
    pub thread_id: String,
    pub subagent_id: String,
    /// `nextByte` of the previous read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_byte: Option<i64>,
}

/// `thread.subagent_steer` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubagentSteerParams {
    pub thread_id: String,
    pub subagent_id: String,
    pub text: String,
}

/// Params naming one subagent of a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SubagentRef {
    pub thread_id: String,
    pub subagent_id: String,
}

/// `thread.goal` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GoalParams {
    pub thread_id: String,
    #[schemars(with = "Value")]
    pub op: GoalOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
}

/// `thread.btw` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BtwParams {
    pub thread_id: String,
    pub question: String,
    /// Continue this side-question topic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record_id: Option<String>,
}

/// `thread.bash` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BashParams {
    pub thread_id: String,
    pub command: String,
    /// Keep the command and its output out of the model's context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_from_context: Option<bool>,
}

/// `thread.git` result: the git status of every repository of the thread's workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGit {
    pub workspace_id: String,
    /// In the order of the workspace's `repos`.
    pub repos: Vec<RepoStatus>,
}

/// `interaction.respond` params (DMN-051).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RespondParams {
    pub thread_id: String,
    pub interaction_id: String,
    pub response: InteractionResponse,
}

/// Answer to an interactive request; on the wire an object with exactly one of these keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "ResponseRepr", into = "ResponseRepr")]
pub enum InteractionResponse {
    /// Answers `select`, `input` or `editor`.
    Value(String),
    /// Answers `confirm`.
    Confirmed(bool),
    /// Dismisses the request.
    Cancelled,
    /// Answers `ask`: one entry per question, in order (DMN-054).
    Answers(Vec<AskAnswer>),
}

// Wire form of `InteractionResponse`.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[schemars(
    description = "Answer to an interactive request: exactly one of `value`, `confirmed`, `cancelled` (always true) or `answers`."
)]
enum ResponseRepr {
    Value(String),
    Confirmed(bool),
    Cancelled(#[schemars(with = "ConstTrue")] LitTrue),
    Answers(#[schemars(with = "Vec<Value>")] Vec<AskAnswer>),
}

impl From<ResponseRepr> for InteractionResponse {
    fn from(repr: ResponseRepr) -> Self {
        match repr {
            ResponseRepr::Value(value) => Self::Value(value),
            ResponseRepr::Confirmed(confirmed) => Self::Confirmed(confirmed),
            ResponseRepr::Cancelled(LitTrue) => Self::Cancelled,
            ResponseRepr::Answers(answers) => Self::Answers(answers),
        }
    }
}

impl From<InteractionResponse> for ResponseRepr {
    fn from(response: InteractionResponse) -> Self {
        match response {
            InteractionResponse::Value(value) => Self::Value(value),
            InteractionResponse::Confirmed(confirmed) => Self::Confirmed(confirmed),
            InteractionResponse::Cancelled => Self::Cancelled(LitTrue),
            InteractionResponse::Answers(answers) => Self::Answers(answers),
        }
    }
}

impl JsonSchema for InteractionResponse {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "InteractionResponse".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ResponseRepr::json_schema(generator)
    }
}

/// Schema of the constant `true`.
struct ConstTrue;

impl JsonSchema for ConstTrue {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ConstTrue".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "const": true })
    }
}

/// `terminal.open` params (05-terminal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenTerminalParams {
    pub kind: TerminalKind,
    /// Required for `omp`; for `shell`, the thread whose home is the cwd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    /// For `shell`: the workspace used as cwd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub cols: u16,
    pub rows: u16,
}

/// `terminal.open` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenTerminalResult {
    pub terminal_id: String,
    /// Binary-frame channel of this terminal on this connection.
    pub channel: u32,
}

/// `terminal.resize` params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResizeTerminalParams {
    pub terminal_id: String,
    pub cols: u16,
    pub rows: u16,
}

/// Params naming one terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRef {
    pub terminal_id: String,
}

/// `terminal.list` params; filters combine.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListTerminalsParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
}

/// `terminal.list` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalList {
    pub terminals: Vec<TerminalInfo>,
}

/// `settings.set` params: values merged into the config file (DMN-009); absent ones are unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetSettingsParams {
    /// `[ui]` values to set; absent fields are unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiSettings>,
    /// Bindings replacing the ones with the same mode and keys (`command: ""` unbinds the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keymap: Option<Vec<KeymapEntry>>,
}
