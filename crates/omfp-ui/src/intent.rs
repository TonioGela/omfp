//! Intents out: every user action as a value. The host maps each [`Intent`] to protocol calls and reports what the UI
//! must know back as an [`IntentOutcome`] in [`crate::ViewInput::outcomes`]; everything else comes back through the
//! topics. Agents can later drive the same intents (D-17).

use omfp_core::wire::{ImageContent, QueuedMessageQueue, ThinkingLevel};
use omfp_protocol::{InteractionResponse, PromptBehavior};

/// One user action.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// The user opened a thread: subscribe to `thread/<id>` and fetch its [`crate::ThreadDetails`].
    SelectThread { thread_id: String },
    /// Creates the requested workspaces in order, then `threads.create` with all of them (UI-023).
    CreateThread(NewThread),
    /// `threads.update`; `None` fields stay unchanged.
    UpdateThread {
        thread_id: String,
        title: Option<String>,
        tags: Option<Vec<String>>,
    },
    /// `threads.attach`, appended.
    AttachWorkspace {
        thread_id: String,
        workspace_id: String,
    },
    /// `threads.detach`.
    DetachWorkspace {
        thread_id: String,
        workspace_id: String,
    },
    /// `repos.add`.
    AddRepository { path: String },
    /// `workspaces.create`, `workspaces.from_pr` or `workspaces.adopt`, without a thread.
    CreateWorkspace(WorkspaceSource),
    /// `workspaces.remove`.
    RemoveWorkspace {
        workspace_id: String,
        force: bool,
        delete_branch: bool,
    },
    /// `workspaces.status`, stored in [`crate::ViewInput::git`].
    RefreshWorkspace { workspace_id: String },
    /// `thread.prompt`; `behavior` is set when the thread is busy (UI-061).
    Prompt {
        thread_id: String,
        text: String,
        images: Vec<ImageContent>,
        behavior: Option<PromptBehavior>,
    },
    /// `thread.abort`; the outcome carries the restored queue (UI-063).
    Abort { thread_id: String },
    /// `thread.queue.remove` or `thread.queue.promote` (UI-062).
    Queue {
        thread_id: String,
        op: QueueOp,
        message: String,
        queue: QueuedMessageQueue,
    },
    /// `interaction.respond`.
    Respond {
        thread_id: String,
        interaction_id: String,
        response: InteractionResponse,
    },
    /// `thread.set_model`.
    SetModel {
        thread_id: String,
        provider: String,
        model_id: String,
    },
    /// `thread.set_thinking`.
    SetThinking {
        thread_id: String,
        level: ThinkingLevel,
    },
    /// `thread.stop`.
    Stop { thread_id: String },
    /// `thread.restart`.
    Restart { thread_id: String },
}

/// What a queue chip action asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QueueOp {
    Remove,
    /// Moves a follow-up ahead as steering.
    Promote,
}

/// The new-thread dialog's result (UI-023).
#[derive(Debug, Clone, PartialEq)]
pub struct NewThread {
    pub title: String,
    pub tags: Vec<String>,
    /// Attached in this order; the first one is the thread's cwd. May be empty.
    pub workspaces: Vec<WorkspaceSource>,
    /// First message, sent once the thread exists.
    pub prompt: Option<String>,
}

/// Where a workspace comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSource {
    /// An existing workspace.
    Existing { workspace_id: String },
    /// A new managed worktree (`workspaces.create`); `repo` is a registered repository id or a path.
    NewWorktree {
        repo: String,
        branch: Option<String>,
        base_ref: Option<String>,
    },
    /// A worktree on a pull request's head (`workspaces.from_pr`).
    FromPr { pr: String, repo: Option<String> },
    /// An existing directory (`workspaces.adopt`).
    Adopt { path: String },
}

/// What the host reports back about a dispatched intent.
#[derive(Debug, Clone, PartialEq)]
pub enum IntentOutcome {
    /// The intent failed; `message` is shown to the user.
    Failed { intent: Intent, message: String },
    /// `threads.create` succeeded: the UI opens the thread.
    ThreadCreated { thread_id: String },
    /// `thread.abort` returned the queued texts, to put back in the composer (UI-063).
    Aborted {
        thread_id: String,
        steering: Vec<String>,
        follow_up: Vec<String>,
    },
    /// The daemon answered a queue action; `applied` is omp's `removed` / `promoted` (UI-062).
    Queue {
        thread_id: String,
        op: QueueOp,
        message: String,
        applied: bool,
    },
}
