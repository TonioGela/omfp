//! Request handlers: every protocol method mapped to the daemon's services (ARC-007), except the connection-level
//! ones (`system.hello`, `subscribe`, `unsubscribe`).

use std::path::PathBuf;
use std::sync::Arc;

use omfp_core::wire::{ImageContent, ThinkingLevel};
use omfp_protocol::{
    Call, CreateWorkspaceParams, Empty, ErrorBody, PathCompletions, PathEntry, PromptBehavior,
    PromptResult, RepoSuggestions, SeenRepository, ThreadGit, ThreadSearchResult,
    ThreadSearchResults, ThreadWorkspace,
};
use omp_rpc::{
    BashCommand, BtwCommand, CancelSubagentCommand, CompactCommand, FollowUpCommand, ForkCommand,
    GetSubagentMessagesCommand, GoalCommand, NavigateTreeCommand, PromoteQueuedMessageCommand,
    PromptCommand, QueuedMessageQueue, RemoveQueuedMessageCommand, SetLabelCommand,
    SetModelCommand, SetThinkingLevelCommand, SteerCommand, SteerSubagentCommand,
    StreamingBehavior,
};
use serde::Serialize;
use serde_json::{Value, json};

use super::auth::Principal;
use super::convert::{
    bad_request, diff_page, internal, supervisor_error, thread_error, workspace_error,
};
use super::services::Services;
use crate::agent::OmpCall;
use crate::store::{Provenance, TitleSource};
use crate::threads::NewThread;
use crate::workspaces::{NewWorkspace, RepoSource, WorkspaceError};
use crate::{fs_complete, search, titles};

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

/// A `workspaces.add_repo` source in the daemon's terms; PR sources wait for S-08.
fn repo_source(source: omfp_protocol::RepoSource) -> Result<RepoSource, ErrorBody> {
    Ok(match source {
        omfp_protocol::RepoSource::Link { path } => RepoSource::Link(PathBuf::from(path)),
        omfp_protocol::RepoSource::Worktree {
            repo,
            branch,
            base_ref,
        } => RepoSource::Worktree {
            repo: PathBuf::from(repo),
            branch,
            base_ref,
        },
        omfp_protocol::RepoSource::Clone { url } => RepoSource::Clone(url),
        omfp_protocol::RepoSource::Pr { .. } => {
            return Err(workspace_error(WorkspaceError::Unsupported(
                "adding a pull request (S-08)",
            )));
        }
    })
}

/// Runs a method for `principal`. Authorization already happened (PRT-034).
pub(crate) async fn dispatch(services: &Services, principal: &Principal, call: Call) -> Reply {
    match call {
        Call::SystemHello(_) | Call::Subscribe(_) | Call::Unsubscribe(_) => {
            Err(bad_request("handled by the connection"))
        }
        Call::ReposSuggest(params) => {
            let workspaces = Arc::clone(&services.workspaces);
            let query = params.query.unwrap_or_default();
            let repos =
                blocking(move || workspaces.suggest(&query).map_err(workspace_error)).await?;
            ok(&RepoSuggestions {
                repos: repos
                    .into_iter()
                    .map(|repo| SeenRepository {
                        path: repo.path.to_string_lossy().into_owned(),
                        name: repo.name,
                        last_seen_at: repo.last_seen_at,
                    })
                    .collect(),
            })
        }
        Call::FsComplete(params) => {
            let entries = blocking(move || {
                let home = std::env::var_os("HOME").map(PathBuf::from);
                Ok(fs_complete::complete(&params.path, home.as_deref()))
            })
            .await?;
            ok(&PathCompletions {
                entries: entries
                    .into_iter()
                    .map(|entry| PathEntry {
                        name: entry.name,
                        path: entry.path.to_string_lossy().into_owned(),
                        is_git_root: entry.is_git_root,
                    })
                    .collect(),
            })
        }
        Call::ThreadsSearch(params) => {
            let store = Arc::clone(&services.store);
            let hits = blocking(move || {
                let threads = store.threads()?;
                Ok(search::search(
                    threads
                        .iter()
                        .map(|thread| (thread.id.as_str(), thread.session_dir.as_path())),
                    &params.query,
                ))
            })
            .await?;
            ok(&ThreadSearchResults {
                results: hits
                    .into_iter()
                    .map(|hit| ThreadSearchResult {
                        thread_id: hit.thread_id,
                        snippet: hit.snippet,
                    })
                    .collect(),
            })
        }
        call @ (Call::WorkspacesCreate(_)
        | Call::WorkspacesUpdate(_)
        | Call::WorkspacesRemove(_)
        | Call::WorkspacesRescan(_)
        | Call::WorkspacesAddRepo(_)
        | Call::WorkspacesRemoveRepo(_)
        | Call::WorkspacesStatus(_)
        | Call::WorkspacesDiff(_)) => workspaces(services, principal, call).await,
        call @ (Call::ThreadsCreate(_)
        | Call::ThreadsUpdate(_)
        | Call::ThreadsArchive(_)
        | Call::ThreadsUnarchive(_)
        | Call::ThreadsDelete(_)
        | Call::ThreadGit(_)) => threads(services, principal, call).await,
        Call::TerminalOpen(_)
        | Call::TerminalResize(_)
        | Call::TerminalClose(_)
        | Call::TerminalList(_) => Err(bad_request("terminals are not available yet (M6)")),
        Call::ThemeGet(_) => ok(&services.theme()),
        Call::ThemeSet(params) => {
            services.set_theme(params.css).await?;
            empty()
        }
        Call::SettingsGet(_) => ok(&services.settings()),
        Call::SettingsSet(params) => ok(&services.set_settings(params).await?),
        call => thread(services, principal, call).await,
    }
}

/// Creates a workspace as `workspaces.create` does; `fallback_name` names it when the params do not.
async fn create_workspace(
    services: &Services,
    params: CreateWorkspaceParams,
    fallback_name: Option<String>,
    created_by: Provenance,
    client: String,
) -> Result<String, ErrorBody> {
    let workspaces = Arc::clone(&services.workspaces);
    let store = Arc::clone(&services.store);
    let new = NewWorkspace {
        name: params.name,
        fallback_name,
        link: params.link.map(PathBuf::from),
        created_by,
    };
    let id = blocking(move || {
        let view = workspaces.create(new).map_err(workspace_error)?;
        store.set_created_client(&view.workspace.id, &client)?;
        Ok(view.workspace.id)
    })
    .await?;
    services.refresh_index_quietly().await;
    Ok(id)
}

#[allow(clippy::too_many_lines)] // One arm per method.
async fn workspaces(services: &Services, principal: &Principal, call: Call) -> Reply {
    let workspaces = Arc::clone(&services.workspaces);
    let id = match call {
        Call::WorkspacesCreate(params) => {
            let client = principal.client().to_owned();
            create_workspace(services, params, None, principal.created_by(), client).await?
        }
        Call::WorkspacesUpdate(params) => {
            let id = params.workspace_id.clone();
            blocking(move || {
                workspaces
                    .rename(&params.workspace_id, &params.name)
                    .map_err(workspace_error)
            })
            .await?;
            id
        }
        Call::WorkspacesRemove(params) => {
            blocking(move || {
                workspaces
                    .remove(&params.workspace_id, params.force)
                    .map_err(workspace_error)
            })
            .await?;
            services.refresh_index_quietly().await;
            return empty();
        }
        Call::WorkspacesRescan(params) => params.workspace_id,
        Call::WorkspacesAddRepo(params) => {
            let id = params.workspace_id.clone();
            let source = repo_source(params.source)?;
            blocking(move || {
                workspaces
                    .add_repo(&params.workspace_id, params.name.as_deref(), source)
                    .map_err(workspace_error)
            })
            .await?;
            id
        }
        Call::WorkspacesRemoveRepo(params) => {
            let id = params.workspace_id.clone();
            blocking(move || {
                workspaces
                    .remove_repo(
                        &params.workspace_id,
                        &params.name,
                        params.force,
                        params.delete_branch,
                    )
                    .map_err(workspace_error)
            })
            .await?;
            id
        }
        Call::WorkspacesStatus(params) => {
            services.rescan(&params.workspace_id).await?;
            let repos = services.workspace_status(&params.workspace_id).await?;
            return match repos.into_iter().find(|repo| repo.name == params.repo) {
                Some(repo) => ok(&repo.status),
                None => Err(workspace_error(WorkspaceError::NoSuchRepo {
                    workspace: params.workspace_id,
                    name: params.repo,
                })),
            };
        }
        Call::WorkspacesDiff(params) => {
            services.rescan(&params.workspace_id).await?;
            let (workspace_id, repo) = (params.workspace_id.clone(), params.repo.clone());
            let diff = blocking(move || {
                let repo = workspaces
                    .repo(&workspace_id, &repo)
                    .map_err(workspace_error)?;
                workspaces.diff(&repo).map_err(workspace_error)
            })
            .await?;
            return ok(&diff_page(
                &diff,
                params.path.as_deref(),
                params.cursor.as_deref(),
            )?);
        }
        _ => return Err(internal("not a workspace method")),
    };
    // Every workspace method rescans (DMN-023); a change reaches the index and the live threads' roots.
    services.rescan(&id).await?;
    ok(&services.workspace_summary(&id).await?)
}

/// `threads.create`: the thread in an existing workspace or a new one named after its title (else its first message),
/// then its first message. Without a title the thread's title comes from the conversation (DMN-073).
async fn create_thread(
    services: &Services,
    principal: &Principal,
    params: omfp_protocol::CreateThreadParams,
) -> Reply {
    let client = principal.client().to_owned();
    let title = params
        .title
        .map(|title| title.trim().to_owned())
        .filter(|title| !title.is_empty());
    let (workspace_id, created) = match params.workspace {
        ThreadWorkspace::Id(id) => (id, false),
        ThreadWorkspace::New(new) => {
            let fallback = title.clone().or_else(|| {
                let words = titles::placeholder(params.prompt.as_deref()?);
                Some(words.trim_end_matches('…').to_owned())
            });
            let id = create_workspace(
                services,
                new,
                fallback,
                principal.created_by(),
                client.clone(),
            )
            .await?;
            (id, true)
        }
    };
    let new = NewThread {
        title_source: if title.is_some() {
            TitleSource::Set
        } else {
            TitleSource::Placeholder
        },
        title: title.unwrap_or_default(),
        workspace_id: workspace_id.clone(),
        model: params.model,
        thinking_level: params.thinking_level.map(thinking_name).transpose()?,
        approval_mode: params.approval_mode,
        created_by: principal.created_by(),
    };
    let threads = Arc::clone(&services.threads);
    let store = Arc::clone(&services.store);
    let created_thread = blocking(move || {
        let thread = threads.create(new).map_err(thread_error)?;
        store.set_created_client(&thread.id, &client)?;
        Ok(thread)
    })
    .await;
    let thread = match created_thread {
        Ok(thread) => thread,
        Err(error) => {
            if created {
                // Best effort: the new folder had no other use.
                let workspaces = Arc::clone(&services.workspaces);
                let _ = blocking(move || {
                    workspaces
                        .remove(&workspace_id, false)
                        .map_err(workspace_error)
                })
                .await;
                services.refresh_index_quietly().await;
            }
            return Err(error);
        }
    };
    if let Some(text) = params.prompt {
        let command = PromptCommand {
            message: text,
            images: None,
            streaming_behavior: None,
        };
        if let Err(error) = services.supervisor.prompt(&thread.id, command).await {
            services.refresh_index_quietly().await;
            // The thread exists; say which one so the client can retry the prompt.
            return Err(supervisor_error(error).with_details(json!({ "threadId": thread.id })));
        }
    }
    ok(&services.thread_summary(&thread.id).await?)
}

async fn threads(services: &Services, principal: &Principal, call: Call) -> Reply {
    let threads = Arc::clone(&services.threads);
    let supervisor = &services.supervisor;
    match call {
        Call::ThreadsCreate(params) => create_thread(services, principal, params).await,
        Call::ThreadsUpdate(params) => {
            let id = params.thread_id.clone();
            if let Some(title) = params.title {
                supervisor
                    .set_title(&id, title.trim().to_owned(), TitleSource::Set)
                    .await
                    .map_err(supervisor_error)?;
            }
            let approval_mode = params.approval_mode;
            let thread_id = id.clone();
            blocking(move || {
                if let Some(mode) = &approval_mode {
                    threads
                        .set_approval_mode(&thread_id, mode)
                        .map_err(thread_error)?;
                }
                threads.get(&thread_id).map_err(thread_error)
            })
            .await?;
            ok(&services.thread_summary(&id).await?)
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
        Call::ThreadsUnarchive(params) => {
            blocking(move || threads.unarchive(&params.thread_id).map_err(thread_error)).await?;
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
            services.rescan(&thread.workspace_id).await?;
            let repos = services.workspace_status(&thread.workspace_id).await?;
            ok(&ThreadGit {
                workspace_id: thread.workspace_id,
                repos,
            })
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
