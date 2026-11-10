//! Intents out: every [`Intent`] becomes protocol calls; what the UI must know comes back as [`Effect`]s.
//!
//! Selecting a thread moves the window's thread subscription (the previous thread is unsubscribed, so the daemon
//! knows nobody watches it any more and may reap its idle process, DMN-034) and fetches the thread's details.

use futures::future::join4;
use omfp_client::{CallError, Client};
use omfp_core::wire::{ModelInfo, RestoredQueuedMessage, ThinkingLevel};
use omfp_protocol::method::{
    InteractionRespond, ReposAdd, ThreadAbort, ThreadGit, ThreadModels, ThreadPrompt,
    ThreadQueuePromote, ThreadQueueRemove, ThreadRestart, ThreadSetModel, ThreadSetThinking,
    ThreadState, ThreadStop, ThreadThinkingLevels, ThreadsAttach, ThreadsCreate, ThreadsDetach,
    ThreadsUpdate, WorkspacesAdopt, WorkspacesCreate, WorkspacesFromPr, WorkspacesRemove,
    WorkspacesStatus,
};
use omfp_protocol::{
    AddRepositoryParams, AdoptWorkspaceParams, AttachWorkspaceParams, Call, CreateThreadParams,
    CreateWorkspaceParams, DetachWorkspaceParams, Method, PromptParams, QueueMessageParams,
    RemoveWorkspaceParams, RespondParams, SetModelParams, SetThinkingParams, ThreadRef, Topic,
    UpdateThreadParams, WorkspaceFromPrParams, WorkspaceRef,
};
use serde_json::Value;

use super::view::Effect;
use crate::intent::{Intent, IntentOutcome, NewThread, QueueOp, WorkspaceSource};

/// The protocol as the dispatcher uses it; implemented by [`Client`].
pub trait Rpc: Clone + 'static {
    /// Sends `call` and resolves to its result; the future does not borrow `self`.
    fn call(&self, call: Call) -> impl Future<Output = Result<Value, CallError>> + 'static;
    fn subscribe(&self, topic: Topic);
    fn unsubscribe(&self, topic: Topic);
}

impl Rpc for Client {
    fn call(&self, call: Call) -> impl Future<Output = Result<Value, CallError>> + 'static {
        Client::call(self, call)
    }

    fn subscribe(&self, topic: Topic) {
        Client::subscribe(self, topic);
    }

    fn unsubscribe(&self, topic: Topic) {
        Client::unsubscribe(self, topic);
    }
}

/// `M` with typed params and result.
async fn request<M: Method>(rpc: &impl Rpc, params: M::Params) -> Result<M::Result, CallError> {
    let value = rpc.call(M::call(params)).await?;
    serde_json::from_value(value)
        .map_err(|error| CallError::Codec(format!("invalid {} result: {error}", M::NAME)))
}

/// Maps intents to calls for one window.
#[derive(Debug)]
pub struct Dispatcher<R> {
    rpc: R,
    /// The thread whose topic this window follows.
    watched: Option<String>,
}

impl<R: Rpc> Dispatcher<R> {
    /// The thread whose topic this window follows.
    #[must_use]
    pub fn watched(&self) -> Option<&str> {
        self.watched.as_deref()
    }

    pub fn new(rpc: R) -> Self {
        Self { rpc, watched: None }
    }

    /// Starts `intent`: subscriptions change at once, calls run in the returned future, which resolves to the effects
    /// to apply to the view input.
    pub fn dispatch(&mut self, intent: Intent) -> impl Future<Output = Vec<Effect>> + 'static {
        if let Intent::SelectThread { thread_id } = &intent {
            self.watch(thread_id);
        }
        let rpc = self.rpc.clone();
        async move {
            match run(&rpc, &intent).await {
                Ok(effects) => effects,
                Err(Failure { message, effects }) => {
                    let mut effects = effects;
                    effects.push(Effect::Outcome(IntentOutcome::Failed { intent, message }));
                    effects
                }
            }
        }
    }

    /// Follows `thread_id` instead of the previously watched thread.
    fn watch(&mut self, thread_id: &str) {
        if self.watched.as_deref() == Some(thread_id) {
            return;
        }
        if let Some(previous) = self.watched.replace(thread_id.to_owned()) {
            self.rpc.unsubscribe(Topic::Thread(previous));
        }
        self.rpc.subscribe(Topic::Thread(thread_id.to_owned()));
    }
}

/// A failed intent: the message shown to the user, and what succeeded before the failure.
struct Failure {
    message: String,
    effects: Vec<Effect>,
}

impl From<CallError> for Failure {
    fn from(error: CallError) -> Self {
        Self {
            message: error.to_string(),
            effects: Vec::new(),
        }
    }
}

type Effects = Result<Vec<Effect>, Failure>;

fn thread_ref(thread_id: &str) -> ThreadRef {
    ThreadRef {
        thread_id: thread_id.to_owned(),
    }
}

/// Runs one intent; every arm names its calls, so a new intent fails to compile until it is mapped.
async fn run(rpc: &impl Rpc, intent: &Intent) -> Effects {
    let call = match intent.clone() {
        Intent::SelectThread { thread_id } => return open_thread(rpc, &thread_id).await,
        Intent::CreateThread(new) => return create_thread(rpc, new).await,
        Intent::CreateWorkspace(source) => return workspace_only(rpc, source).await,
        Intent::Abort { thread_id } => return abort(rpc, thread_id).await,
        Intent::Queue {
            thread_id,
            op,
            message,
            queue,
        } => {
            let params = QueueMessageParams {
                thread_id,
                message,
                queue: Some(queue),
            };
            return queue_op(rpc, op, params).await;
        }
        Intent::RefreshWorkspace { workspace_id } => return refresh_git(rpc, workspace_id).await,
        Intent::SetModel {
            thread_id,
            provider,
            model_id,
        } => {
            let call = ThreadSetModel::call(SetModelParams {
                thread_id: thread_id.clone(),
                provider,
                model_id,
            });
            return change_session(rpc, thread_id, call).await;
        }
        Intent::SetThinking { thread_id, level } => {
            let call = ThreadSetThinking::call(SetThinkingParams {
                thread_id: thread_id.clone(),
                level,
            });
            return change_session(rpc, thread_id, call).await;
        }
        // Calls whose results the UI does not need: their effects come back through the topics.
        Intent::UpdateThread {
            thread_id,
            title,
            tags,
        } => ThreadsUpdate::call(UpdateThreadParams {
            thread_id,
            title,
            tags,
        }),
        Intent::AttachWorkspace {
            thread_id,
            workspace_id,
        } => ThreadsAttach::call(AttachWorkspaceParams {
            thread_id,
            workspace_id,
            position: None,
        }),
        Intent::DetachWorkspace {
            thread_id,
            workspace_id,
        } => ThreadsDetach::call(DetachWorkspaceParams {
            thread_id,
            workspace_id,
        }),
        Intent::AddRepository { path } => ReposAdd::call(AddRepositoryParams { path }),
        Intent::RemoveWorkspace {
            workspace_id,
            force,
            delete_branch,
        } => WorkspacesRemove::call(RemoveWorkspaceParams {
            workspace_id,
            force,
            delete_branch,
        }),
        Intent::Prompt {
            thread_id,
            text,
            images,
            behavior,
        } => ThreadPrompt::call(PromptParams {
            thread_id,
            text,
            images,
            behavior,
        }),
        Intent::Respond {
            thread_id,
            interaction_id,
            response,
        } => InteractionRespond::call(RespondParams {
            thread_id,
            interaction_id,
            response,
        }),
        Intent::Stop { thread_id } => ThreadStop::call(thread_ref(&thread_id)),
        Intent::Restart { thread_id } => ThreadRestart::call(thread_ref(&thread_id)),
    };
    rpc.call(call).await?;
    Ok(Vec::new())
}

/// `workspaces.status`.
async fn refresh_git(rpc: &impl Rpc, workspace_id: String) -> Effects {
    let params = WorkspaceRef {
        workspace_id: workspace_id.clone(),
    };
    let status = request::<WorkspacesStatus>(rpc, params).await?;
    Ok(vec![Effect::Git {
        workspace_id,
        status: Some(status),
    }])
}

/// A model or thinking-level change, then `thread.state` again so the header shows it.
async fn change_session(rpc: &impl Rpc, thread_id: String, call: Call) -> Effects {
    rpc.call(call).await?;
    refresh_session(rpc, thread_id).await
}

/// `thread.abort`: the restored queue texts go back to the composer (UI-063).
async fn abort(rpc: &impl Rpc, thread_id: String) -> Effects {
    let restored = request::<ThreadAbort>(rpc, thread_ref(&thread_id)).await?;
    let texts = |messages: Vec<RestoredQueuedMessage>| {
        messages.into_iter().map(|message| message.text).collect()
    };
    Ok(vec![Effect::Outcome(IntentOutcome::Aborted {
        thread_id,
        steering: texts(restored.steering),
        follow_up: texts(restored.follow_up),
    })])
}

/// `thread.queue.remove` / `thread.queue.promote`, with omp's answer (UI-062).
async fn queue_op(rpc: &impl Rpc, op: QueueOp, params: QueueMessageParams) -> Effects {
    let (thread_id, message) = (params.thread_id.clone(), params.message.clone());
    let applied = match op {
        QueueOp::Remove => request::<ThreadQueueRemove>(rpc, params).await?.removed,
        QueueOp::Promote => request::<ThreadQueuePromote>(rpc, params).await?.promoted,
    };
    Ok(vec![Effect::Outcome(IntentOutcome::Queue {
        thread_id,
        op,
        message,
        applied,
    })])
}

/// A list result. The daemon forwards omp-rpc's decoded output, a bare array, for `thread.models` and
/// `thread.thinking_levels`, although the method table names omp's `{models}` / `{levels}` wrappers.
async fn bare_list<T: serde::de::DeserializeOwned>(
    rpc: &impl Rpc,
    call: Call,
) -> Result<Vec<T>, CallError> {
    let method = call.method();
    let value = rpc.call(call).await?;
    serde_json::from_value(value)
        .map_err(|error| CallError::Codec(format!("invalid {method} result: {error}")))
}

/// The thread's details (`thread.state`, `thread.models`, `thread.thinking_levels`) and its workspaces' git status.
/// Whatever arrived is applied; the first error is reported.
async fn open_thread(rpc: &impl Rpc, thread_id: &str) -> Effects {
    let (session, models, levels, git) = join4(
        request::<ThreadState>(rpc, thread_ref(thread_id)),
        bare_list::<ModelInfo>(rpc, ThreadModels::call(thread_ref(thread_id))),
        bare_list::<ThinkingLevel>(rpc, ThreadThinkingLevels::call(thread_ref(thread_id))),
        request::<ThreadGit>(rpc, thread_ref(thread_id)),
    )
    .await;
    let mut errors = Vec::new();
    let session = kept(&mut errors, session).map(Box::new);
    let models = kept(&mut errors, models);
    let thinking_levels = kept(&mut errors, levels);
    let git = kept(&mut errors, git);
    let mut effects = vec![Effect::Details {
        thread_id: thread_id.to_owned(),
        session,
        models,
        thinking_levels,
    }];
    effects.extend(
        git.into_iter()
            .flat_map(|git| git.workspaces)
            .map(|workspace| Effect::Git {
                workspace_id: workspace.workspace_id,
                status: workspace.status,
            }),
    );
    match errors.into_iter().next() {
        None => Ok(effects),
        Some(message) => Err(Failure { message, effects }),
    }
}

/// The value of `result`, or `None` with its error recorded.
fn kept<T>(errors: &mut Vec<String>, result: Result<T, CallError>) -> Option<T> {
    result.map_err(|error| errors.push(error.to_string())).ok()
}

/// `thread.state` again after a change of model or thinking level.
async fn refresh_session(rpc: &impl Rpc, thread_id: String) -> Effects {
    let session = request::<ThreadState>(rpc, thread_ref(&thread_id)).await?;
    Ok(vec![Effect::Details {
        thread_id,
        session: Some(Box::new(session)),
        models: None,
        thinking_levels: None,
    }])
}

/// A workspace without a thread; it shows up through the index.
async fn workspace_only(rpc: &impl Rpc, source: WorkspaceSource) -> Effects {
    create_workspace(rpc, source).await?;
    Ok(Vec::new())
}

/// The id of the workspace `source` names, created first unless it exists.
async fn create_workspace(rpc: &impl Rpc, source: WorkspaceSource) -> Result<String, CallError> {
    let workspace = match source {
        WorkspaceSource::Existing { workspace_id } => return Ok(workspace_id),
        WorkspaceSource::NewWorktree {
            repo,
            branch,
            base_ref,
        } => {
            let params = CreateWorkspaceParams {
                repo,
                branch,
                base_ref,
            };
            request::<WorkspacesCreate>(rpc, params).await?
        }
        WorkspaceSource::FromPr { pr, repo } => {
            request::<WorkspacesFromPr>(rpc, WorkspaceFromPrParams { pr, repo }).await?
        }
        WorkspaceSource::Adopt { path } => {
            request::<WorkspacesAdopt>(rpc, AdoptWorkspaceParams { path }).await?
        }
    };
    Ok(workspace.id)
}

/// Creates the workspaces in order, then the thread with them and its first message (UI-023). When only the first
/// message fails, the thread exists: it is opened and the error shown.
async fn create_thread(rpc: &impl Rpc, new: NewThread) -> Effects {
    let mut workspace_ids = Vec::with_capacity(new.workspaces.len());
    for source in new.workspaces {
        workspace_ids.push(create_workspace(rpc, source).await?);
    }
    let params = CreateThreadParams {
        title: new.title,
        workspace_ids,
        tags: new.tags,
        model: None,
        thinking_level: None,
        approval_mode: None,
        prompt: new.prompt,
    };
    match request::<ThreadsCreate>(rpc, params).await {
        Ok(thread) => Ok(vec![Effect::Outcome(IntentOutcome::ThreadCreated {
            thread_id: thread.id,
        })]),
        Err(CallError::Daemon(error)) => {
            let created = error
                .details
                .as_ref()
                .and_then(|details| details["threadId"].as_str())
                .map(str::to_owned);
            Err(Failure {
                message: CallError::Daemon(error).to_string(),
                effects: created
                    .map(|thread_id| Effect::Outcome(IntentOutcome::ThreadCreated { thread_id }))
                    .into_iter()
                    .collect(),
            })
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests;
