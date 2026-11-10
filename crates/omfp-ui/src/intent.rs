//! Intents out: every user action as a value. The host maps each [`Intent`] to protocol calls and reports what the UI
//! must know back as an [`IntentOutcome`] in [`crate::ViewInput::outcomes`]; everything else comes back through the
//! topics. Agents can later drive the same intents (D-17).

use omfp_core::wire::{ImageContent, QueuedMessageQueue, ThinkingLevel};
use omfp_protocol::{InteractionResponse, PromptBehavior, RepoSource};

/// One user action.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    /// The user opened a thread: subscribe to `thread/<id>` and fetch its [`crate::ThreadDetails`].
    SelectThread { thread_id: String },
    /// `threads.create` in a new workspace, then `workspaces.add_repo` for each repository and the first
    /// `thread.prompt` (UI-023).
    CreateThread(NewThread),
    /// `threads.update`; `None` fields stay unchanged.
    UpdateThread {
        thread_id: String,
        title: Option<String>,
        tags: Option<Vec<String>>,
    },
    /// `threads.archive` (UI-026).
    ArchiveThread { thread_id: String },
    /// `threads.unarchive` (UI-026).
    UnarchiveThread { thread_id: String },
    /// `threads.delete`; the workspaces are kept (UI-026).
    DeleteThread { thread_id: String },
    /// `repos.add`.
    AddRepository { path: String },
    /// `repos.remove`; its workspaces stay (UI-025).
    RemoveRepository { repository_id: String },
    /// `workspaces.add_repo` into `workspace_id`, or into a new workspace (`workspaces.create` first) when `None`.
    AddRepo {
        workspace_id: Option<String>,
        source: RepoSource,
    },
    /// `workspaces.remove_repo`.
    RemoveRepo {
        workspace_id: String,
        name: String,
        force: bool,
        delete_branch: bool,
    },
    /// `workspaces.rescan`, then `workspaces.status` of each repository, stored in [`crate::ViewInput::git`].
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
    /// Added to the thread's new workspace in this order. May be empty.
    pub repos: Vec<RepoSource>,
    /// First message, sent once the repositories are in place.
    pub prompt: Option<String>,
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
