//! Request handlers: every protocol method mapped to the daemon's services (ARC-007), except the connection-level
//! ones (`system.hello`, `subscribe`, `unsubscribe`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use omp_rpc::{
    BashCommand, BtwCommand, CancelSubagentCommand, CompactCommand, FollowUpCommand, ForkCommand,
    GetSubagentMessagesCommand, GoalCommand, NavigateTreeCommand, PromoteQueuedMessageCommand,
    PromptCommand, QueuedMessageQueue, RemoveQueuedMessageCommand, SetLabelCommand,
    SetModelCommand, SetThinkingLevelCommand, SteerCommand, SteerSubagentCommand,
    StreamingBehavior,
};
use pmo_core::wire::{ImageContent, ThinkingLevel};
use pmo_protocol::{
    Call, Empty, ErrorBody, PromptBehavior, PromptResult, RepositoryList, ThreadGit, WorkspaceGit,
};
use serde::Serialize;
use serde_json::{Value, json};

use super::auth::Principal;
use super::convert::{
    self, bad_request, diff_page, internal, supervisor_error, thread_error, workspace_error,
};
use super::services::Services;
use crate::agent::OmpCall;
use crate::threads::NewThread;
use crate::workspaces::{NewWorkspace, RemoveOptions, RepoRef};

type Reply = Result<Value, ErrorBody>;

fn ok(value: &impl Serialize) -> Reply {
    serde_json::to_value(value).map_err(internal)
}

fn empty() -> Reply {
    ok(&Empty {})
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, ErrorBody> + Send + 'static,
) -> Result<T, ErrorBody> {
    tokio::task::spawn_blocking(work).await.map_err(internal)?
}

fn images(images: Vec<ImageContent>) -> Option<Vec<ImageContent>> {
    (!images.is_empty()).then_some(images)
}

/// omp's name of a thinking level (`high`, …), as stored on the thread.
fn thinking_name(level: ThinkingLevel) -> Result<String, ErrorBody> {
    match serde_json::to_value(level).map_err(internal)? {
        Value::String(name) => Ok(name),
        other => Err(internal(format!("unexpected thinking level {other}"))),
    }
}

/// A repository id, or a path when the value is an absolute path (DMN-021).
fn repo_ref(repo: String) -> RepoRef {
    if Path::new(&repo).is_absolute() {
        RepoRef::Path(PathBuf::from(repo))
    } else {
        RepoRef::Id(repo)
    }
}

/// Runs a method for `principal`. Authorization already happened (PRT-034).
pub(crate) async fn dispatch(services: &Services, principal: &Principal, call: Call) -> Reply {
    match call {
        Call::SystemHello(_) | Call::Subscribe(_) | Call::Unsubscribe(_) => {
            Err(bad_request("handled by the connection"))
        }
        call @ (Call::ReposAdd(_)
        | Call::ReposRemove(_)
        | Call::ReposList(_)
        | Call::WorkspacesCreate(_)
        | Call::WorkspacesFromPr(_)
        | Call::WorkspacesAdopt(_)
        | Call::WorkspacesRemove(_)
        | Call::WorkspacesForget(_)
        | Call::WorkspacesStatus(_)
        | Call::WorkspacesDiff(_)) => workspaces(services, principal, call).await,
        call @ (Call::ThreadsCreate(_)
        | Call::ThreadsUpdate(_)
        | Call::ThreadsAttach(_)
        | Call::ThreadsDetach(_)
        | Call::ThreadsArchive(_)
        | Call::ThreadsDelete(_)
        | Call::ThreadGit(_)) => threads(services, principal, call).await,
        Call::TerminalOpen(_)
        | Call::TerminalResize(_)
        | Call::TerminalClose(_)
        | Call::TerminalList(_) => Err(bad_request("terminals are not available yet (M6)")),
        call => thread(services, principal, call).await,
    }
}

#[allow(clippy::too_many_lines)] // One arm per method.
async fn workspaces(services: &Services, principal: &Principal, call: Call) -> Reply {
    let workspaces = Arc::clone(&services.workspaces);
    let store = Arc::clone(&services.store);
    let client = principal.client().to_owned();
    let reply = match call {
        Call::ReposAdd(params) => {
            let repo = blocking(move || {
                workspaces
                    .add_repository(Path::new(&params.path), None)
                    .map_err(workspace_error)
            })
            .await?;
            ok(&convert::repository(&repo))
        }
        Call::ReposRemove(params) => {
            blocking(move || {
                workspaces
                    .remove_repository(&params.repository_id)
                    .map_err(workspace_error)
            })
            .await?;
            empty()
        }
        Call::ReposList(_) => {
            let repositories =
                blocking(move || workspaces.repositories().map_err(workspace_error)).await?;
            return ok(&RepositoryList {
                repositories: repositories.iter().map(convert::repository).collect(),
            });
        }
        Call::WorkspacesCreate(params) => {
            let mut new = NewWorkspace::new(repo_ref(params.repo));
            new.branch = params.branch;
            new.base_ref = params.base_ref;
            new.created_by = principal.created_by();
            let workspace = blocking(move || {
                let workspace = workspaces.create(new).map_err(workspace_error)?;
                store.set_created_client(&workspace.id, &client)?;
                let clients = store.created_clients()?;
                Ok(convert::workspace(&workspace, &clients))
            })
            .await?;
            ok(&workspace)
        }
        Call::WorkspacesFromPr(_) => {
            return Err(bad_request(
                "workspaces from pull requests are not supported yet (M3, DMN-022)",
            ));
        }
        Call::WorkspacesAdopt(params) => {
            let created_by = principal.created_by();
            let workspace = blocking(move || {
                let workspace = workspaces
                    .adopt(Path::new(&params.path), created_by)
                    .map_err(workspace_error)?;
                store.set_created_client(&workspace.id, &client)?;
                let clients = store.created_clients()?;
                Ok(convert::workspace(&workspace, &clients))
            })
            .await?;
            ok(&workspace)
        }
        Call::WorkspacesRemove(params) => {
            let options = RemoveOptions {
                force: params.force,
                delete_branch: params.delete_branch,
                in_use_by_live_thread: services.workspace_in_use(&params.workspace_id).await?,
            };
            blocking(move || {
                workspaces
                    .remove(&params.workspace_id, options)
                    .map_err(workspace_error)
            })
            .await?;
            empty()
        }
        Call::WorkspacesForget(params) => {
            let in_use = services.workspace_in_use(&params.workspace_id).await?;
            blocking(move || {
                workspaces
                    .forget(&params.workspace_id, in_use)
                    .map_err(workspace_error)
            })
            .await?;
            empty()
        }
        Call::WorkspacesStatus(params) => {
            return match services.workspace_status(&params.workspace_id).await? {
                Some(status) => ok(&status),
                None => Err(bad_request(format!(
                    "workspace {} is not a git repository",
                    params.workspace_id
                ))),
            };
        }
        Call::WorkspacesDiff(params) => {
            let id = params.workspace_id.clone();
            let diff = blocking(move || workspaces.diff(&id).map_err(workspace_error)).await?;
            let Some(diff) = diff else {
                return Err(bad_request(format!(
                    "workspace {} is not a git repository",
                    params.workspace_id
                )));
            };
            return ok(&diff_page(
                &diff,
                params.path.as_deref(),
                params.cursor.as_deref(),
            )?);
        }
        _ => return Err(internal("not a workspace method")),
    };
    services.refresh_index_quietly().await;
    reply
}

#[allow(clippy::too_many_lines)] // One arm per method.
async fn threads(services: &Services, principal: &Principal, call: Call) -> Reply {
    let threads = Arc::clone(&services.threads);
    let store = Arc::clone(&services.store);
    let supervisor = &services.supervisor;
    match call {
        Call::ThreadsCreate(params) => {
            let new = NewThread {
                title: params.title,
                workspace_ids: params.workspace_ids,
                tags: params.tags,
                model: params.model,
                thinking_level: params.thinking_level.map(thinking_name).transpose()?,
                approval_mode: params.approval_mode,
                created_by: principal.created_by(),
            };
            let client = principal.client().to_owned();
            let thread = blocking(move || {
                let thread = threads.create(new).map_err(thread_error)?;
                store.set_created_client(&thread.id, &client)?;
                Ok(thread)
            })
            .await?;
            if let Some(text) = params.prompt {
                let command = PromptCommand {
                    message: text,
                    images: None,
                    streaming_behavior: None,
                };
                if let Err(error) = supervisor.prompt(&thread.id, command).await {
                    services.refresh_index_quietly().await;
                    // The thread exists; say which one so the client can retry the prompt.
                    return Err(
                        supervisor_error(error).with_details(json!({ "threadId": thread.id }))
                    );
                }
            }
            ok(&services.thread_summary(&thread.id).await?)
        }
        Call::ThreadsUpdate(params) => {
            let id = params.thread_id.clone();
            blocking(move || {
                if let Some(title) = &params.title {
                    threads
                        .rename(&params.thread_id, title)
                        .map_err(thread_error)?;
                }
                if let Some(tags) = &params.tags {
                    threads
                        .set_tags(&params.thread_id, tags)
                        .map_err(thread_error)?;
                }
                threads.get(&params.thread_id).map_err(thread_error)
            })
            .await?;
            ok(&services.thread_summary(&id).await?)
        }
        Call::ThreadsAttach(params) => {
            let position = params
                .position
                .map(|position| usize::try_from(position).unwrap_or(usize::MAX));
            supervisor
                .attach(&params.thread_id, &params.workspace_id, position)
                .await
                .map_err(supervisor_error)?;
            ok(&services.thread_summary(&params.thread_id).await?)
        }
        Call::ThreadsDetach(params) => {
            supervisor
                .detach(&params.thread_id, &params.workspace_id)
                .await
                .map_err(supervisor_error)?;
            ok(&services.thread_summary(&params.thread_id).await?)
        }
        Call::ThreadsArchive(params) => {
            supervisor
                .stop(&params.thread_id)
                .await
                .map_err(supervisor_error)?;
            blocking(move || threads.archive(&params.thread_id).map_err(thread_error)).await?;
            services.refresh_index_quietly().await;
            empty()
        }
        Call::ThreadsDelete(params) => {
            let id = params.thread_id.clone();
            let reader = Arc::clone(&threads);
            blocking(move || reader.get(&id).map_err(thread_error)).await?;
            supervisor
                .forget(&params.thread_id)
                .await
                .map_err(supervisor_error)?;
            blocking(move || threads.delete(&params.thread_id).map_err(thread_error)).await?;
            services.refresh_index_quietly().await;
            empty()
        }
        Call::ThreadGit(params) => {
            let id = params.thread_id.clone();
            let thread = blocking(move || threads.get(&id).map_err(thread_error)).await?;
            let mut workspaces = Vec::new();
            for workspace_id in thread.workspace_ids {
                let status = services.workspace_status(&workspace_id).await?;
                workspaces.push(WorkspaceGit {
                    workspace_id,
                    status,
                });
            }
            ok(&ThreadGit { workspaces })
        }
        _ => Err(internal("not a threads method")),
    }
}

/// Methods acting on a thread's agent; every omp command starts omp when needed (DMN-070).
#[allow(clippy::too_many_lines)] // One arm per method.
async fn thread(services: &Services, principal: &Principal, call: Call) -> Reply {
    let supervisor = &services.supervisor;
    let forward = |id: String, call: OmpCall| async move {
        supervisor.call(&id, call).await.map_err(supervisor_error)
    };
    match call {
        Call::ThreadPrompt(params) => {
            let behavior = match params.behavior {
                Some(behavior) => Some(behavior),
                // omp needs a behavior while the agent works (02-daemon, "Commands towards omp").
                None if !supervisor.status(&params.thread_id).settled => {
                    Some(PromptBehavior::FollowUp)
                }
                None => None,
            };
            let command = PromptCommand {
                message: params.text,
                images: images(params.images),
                streaming_behavior: behavior.map(|behavior| match behavior {
                    PromptBehavior::Steer => StreamingBehavior::Steer,
                    PromptBehavior::FollowUp => StreamingBehavior::FollowUp,
                }),
            };
            let admission = supervisor
                .prompt(&params.thread_id, command)
                .await
                .map_err(supervisor_error)?;
            ok(&PromptResult {
                prompt_id: admission.prompt_id,
                agent_invoked: admission.agent_invoked,
            })
        }
        Call::ThreadSteer(params) => {
            let command = SteerCommand {
                message: params.text,
                images: images(params.images),
            };
            forward(params.thread_id, OmpCall::Steer(command)).await?;
            empty()
        }
        Call::ThreadFollowUp(params) => {
            let command = FollowUpCommand {
                message: params.text,
                images: images(params.images),
            };
            forward(params.thread_id, OmpCall::FollowUp(command)).await?;
            empty()
        }
        Call::ThreadAbort(params) => forward(params.thread_id, OmpCall::AbortAndRestoreQueue).await,
        Call::ThreadQueueRemove(params) => {
            let command = RemoveQueuedMessageCommand {
                message: params.message,
                queue: params.queue.unwrap_or(QueuedMessageQueue::FollowUp),
            };
            forward(params.thread_id, OmpCall::RemoveQueued(command)).await
        }
        Call::ThreadQueuePromote(params) => {
            let command = PromoteQueuedMessageCommand {
                message: params.message,
            };
            forward(params.thread_id, OmpCall::PromoteQueued(command)).await
        }
        Call::ThreadSetModel(params) => {
            let model = format!("{}/{}", params.provider, params.model_id);
            let command = SetModelCommand {
                provider: params.provider,
                model_id: params.model_id,
            };
            forward(params.thread_id.clone(), OmpCall::SetModel(command)).await?;
            let threads = Arc::clone(&services.threads);
            blocking(move || {
                threads
                    .set_model(&params.thread_id, Some(&model))
                    .map_err(thread_error)
            })
            .await?;
            empty()
        }
        Call::ThreadSetThinking(params) => {
            let name = thinking_name(params.level)?;
            let command = SetThinkingLevelCommand {
                level: params.level,
            };
            forward(params.thread_id.clone(), OmpCall::SetThinkingLevel(command)).await?;
            let threads = Arc::clone(&services.threads);
            blocking(move || {
                threads
                    .set_thinking_level(&params.thread_id, Some(&name))
                    .map_err(thread_error)
            })
            .await?;
            empty()
        }
        Call::ThreadModels(params) => forward(params.thread_id, OmpCall::AvailableModels).await,
        Call::ThreadThinkingLevels(params) => {
            forward(params.thread_id, OmpCall::ThinkingLevels).await
        }
        Call::ThreadCommands(params) => forward(params.thread_id, OmpCall::Commands).await,
        Call::ThreadCompact(params) => {
            let command = CompactCommand {
                custom_instructions: params.instructions,
            };
            forward(params.thread_id, OmpCall::Compact(command)).await
        }
        Call::ThreadTree(params) => forward(params.thread_id, OmpCall::Tree).await,
        Call::ThreadNavigate(params) => {
            let command = NavigateTreeCommand {
                entry_id: params.entry_id,
                summarize: params.summarize,
                custom_instructions: params.instructions,
            };
            forward(params.thread_id, OmpCall::NavigateTree(command)).await
        }
        Call::ThreadFork(params) => {
            let command = ForkCommand {
                entry_id: params.entry_id,
            };
            forward(params.thread_id, OmpCall::Fork(command)).await
        }
        Call::ThreadLabel(params) => {
            let command = SetLabelCommand {
                entry_id: params.entry_id,
                label: params.label,
            };
            forward(params.thread_id, OmpCall::SetLabel(command)).await?;
            empty()
        }
        Call::ThreadSubagents(params) => forward(params.thread_id, OmpCall::Subagents).await,
        Call::ThreadSubagentMessages(params) => {
            let command = GetSubagentMessagesCommand {
                subagent_id: Some(params.subagent_id),
                session_file: None,
                from_byte: params.from_byte,
            };
            forward(params.thread_id, OmpCall::SubagentMessages(command)).await
        }
        Call::ThreadSubagentSteer(params) => {
            let command = SteerSubagentCommand {
                subagent_id: params.subagent_id,
                message: params.text,
            };
            forward(params.thread_id, OmpCall::SteerSubagent(command)).await?;
            empty()
        }
        Call::ThreadSubagentCancel(params) => {
            let command = CancelSubagentCommand {
                subagent_id: params.subagent_id,
            };
            forward(params.thread_id, OmpCall::CancelSubagent(command)).await?;
            empty()
        }
        Call::ThreadGoal(params) => {
            let command = GoalCommand {
                op: params.op,
                objective: params.objective,
                token_budget: params.token_budget,
            };
            forward(params.thread_id, OmpCall::Goal(command)).await
        }
        Call::ThreadBtw(params) => {
            let command = BtwCommand {
                question: params.question,
                record_id: params.record_id,
            };
            forward(params.thread_id, OmpCall::Btw(command)).await
        }
        Call::ThreadBash(params) => {
            let command = BashCommand {
                command: params.command,
                exclude_from_context: params.exclude_from_context,
            };
            forward(params.thread_id, OmpCall::Bash(command)).await
        }
        Call::ThreadState(params) => forward(params.thread_id, OmpCall::State).await,
        Call::ThreadRestart(params) => {
            supervisor
                .restart(&params.thread_id)
                .await
                .map_err(supervisor_error)?;
            empty()
        }
        Call::ThreadStop(params) => {
            supervisor
                .stop(&params.thread_id)
                .await
                .map_err(supervisor_error)?;
            empty()
        }
        Call::InteractionRespond(params) => {
            supervisor
                .respond(
                    &params.thread_id,
                    &params.interaction_id,
                    params.response,
                    principal.client(),
                )
                .await
                .map_err(supervisor_error)?;
            empty()
        }
        other => Err(internal(format!(
            "`{}` is not a thread method",
            other.method()
        ))),
    }
}
