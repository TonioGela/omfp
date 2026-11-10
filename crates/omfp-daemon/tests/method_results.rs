//! Integration tests (L2): every method answers with its declared result type (`Method::Result`, AGT-007), checked
//! by decoding the daemon's response over a real WebSocket. The daemon runs on the fake launcher and fake git.

mod client;

use std::path::Path;
use std::sync::Arc;

use clap::Parser;
use client::{Client, Follower};
use omfp_core::wire::{ExtensionUiRequest, GoalOp, ThinkingLevel};
use omfp_daemon::agent::{FakeLauncher, FakeRun};
use omfp_daemon::config::{Cli, Config};
use omfp_daemon::git::FakeGit;
use omfp_daemon::{DaemonHandle, Ports, start_with};
use omfp_protocol::method::{
    InteractionRespond, ReposAdd, ReposList, ReposRemove, Subscribe, SystemHello, ThreadAbort,
    ThreadBash, ThreadBtw, ThreadCommands, ThreadCompact, ThreadFollowUp, ThreadFork, ThreadGit,
    ThreadGoal, ThreadLabel, ThreadModels, ThreadNavigate, ThreadPrompt, ThreadQueuePromote,
    ThreadQueueRemove, ThreadRestart, ThreadSetModel, ThreadSetThinking, ThreadState, ThreadSteer,
    ThreadStop, ThreadSubagentCancel, ThreadSubagentMessages, ThreadSubagentSteer, ThreadSubagents,
    ThreadThinkingLevels, ThreadTree, ThreadsArchive, ThreadsAttach, ThreadsCreate, ThreadsDelete,
    ThreadsDetach, ThreadsUnarchive, ThreadsUpdate, Unsubscribe, WorkspacesAdopt, WorkspacesCreate,
    WorkspacesDiff, WorkspacesForget, WorkspacesRemove, WorkspacesStatus,
};
use omfp_protocol::{
    AddRepositoryParams, AdoptWorkspaceParams, AttachWorkspaceParams, BashParams, BtwParams,
    CompactParams, CreateThreadParams, CreateWorkspaceParams, DetachWorkspaceParams, Empty,
    ForkParams, GoalParams, HelloParams, InteractionResponse, LabelParams, MessageParams,
    NavigateParams, PromptParams, QueueMessageParams, RemoveWorkspaceParams, RepositoryRef,
    RespondParams, SetModelParams, SetThinkingParams, SubagentMessagesParams, SubagentRef,
    SubagentSteerParams, SubscribeParams, SubscribeResult, ThreadRef, Topic, TopicParams,
    UpdateThreadParams, WorkspaceDiffParams, WorkspaceRef,
};
use tempfile::TempDir;

struct TestDaemon {
    handle: DaemonHandle,
    launcher: FakeLauncher,
    git: Arc<FakeGit>,
    dir: TempDir,
}

async fn daemon() -> TestDaemon {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    let cli = Cli::try_parse_from([
        "daemon",
        "--bind",
        "127.0.0.1:0",
        "--data-dir",
        data_dir.to_str().unwrap(),
    ])
    .unwrap();
    let config = Config::from_cli(cli, None, None).unwrap();
    let launcher = FakeLauncher::new();
    let git = Arc::new(FakeGit::new());
    let ports = Ports {
        launcher: Arc::new(launcher.clone()),
        git: Arc::clone(&git) as _,
        omp_program: "omp".into(),
        omp_version: omfp_core::OMP_VERSION.into(),
        omp_env: Vec::new(),
        warnings: Vec::new(),
    };
    let handle = start_with(config, ports).await.unwrap();
    TestDaemon {
        handle,
        launcher,
        git,
        dir,
    }
}

impl TestDaemon {
    async fn client(&self) -> Client {
        Client::hello(self.handle.addr(), self.handle.token().expose(), "test").await
    }

    /// A new directory under the test's temporary directory.
    fn folder(&self, name: &str) -> String {
        let path = self.dir.path().join(name);
        std::fs::create_dir_all(&path).unwrap();
        path.to_str().unwrap().to_owned()
    }
}

fn fixture(name: &str) -> FakeRun {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/threads")
        .join(omfp_core::OMP_VERSION)
        .join(format!("{name}.jsonl"));
    FakeRun::from_fixture(&path).unwrap()
}

fn new_thread(title: &str) -> CreateThreadParams {
    CreateThreadParams {
        title: title.into(),
        workspace_ids: Vec::new(),
        tags: Vec::new(),
        model: None,
        thinking_level: None,
        approval_mode: None,
        prompt: None,
    }
}

fn thread_ref(thread_id: &str) -> ThreadRef {
    ThreadRef {
        thread_id: thread_id.into(),
    }
}

fn message(thread_id: &str, text: &str) -> MessageParams {
    MessageParams {
        thread_id: thread_id.into(),
        text: text.into(),
        images: Vec::new(),
    }
}

// spec: AGT-007
#[tokio::test(flavor = "multi_thread")]
async fn connection_repository_and_workspace_methods_return_their_declared_results() {
    let daemon = daemon().await;
    let mut client = Client::connect(daemon.handle.addr()).await;
    let hello = client
        .typed::<SystemHello>(HelloParams {
            token: daemon.handle.token().expose().to_owned(),
            protocol: omfp_protocol::PROTOCOL_VERSION,
            client: "test".into(),
        })
        .await;
    assert_eq!(hello.protocol, omfp_protocol::PROTOCOL_VERSION);
    let subscribed = client
        .typed::<Subscribe>(SubscribeParams {
            topic: Topic::Index,
            since: None,
        })
        .await;
    assert!(matches!(subscribed, SubscribeResult::Snapshot { .. }));
    client
        .typed::<Unsubscribe>(TopicParams {
            topic: Topic::Index,
        })
        .await;

    let root = daemon.folder("repo");
    daemon.git.add_repo(Path::new(&root), "main");
    let repository = client
        .typed::<ReposAdd>(AddRepositoryParams { path: root })
        .await;
    let listed = client.typed::<ReposList>(Empty {}).await;
    assert_eq!(listed.repositories, std::slice::from_ref(&repository));
    let managed = client
        .typed::<WorkspacesCreate>(CreateWorkspaceParams {
            repo: repository.id.clone(),
            branch: Some("feature".into()),
            base_ref: Some("main".into()),
        })
        .await;
    let adopted = client
        .typed::<WorkspacesAdopt>(AdoptWorkspaceParams {
            path: daemon.folder("plain"),
        })
        .await;
    let status = client
        .typed::<WorkspacesStatus>(WorkspaceRef {
            workspace_id: managed.id.clone(),
        })
        .await;
    assert_eq!(status.branch.as_deref(), Some("feature"));
    client
        .typed::<WorkspacesDiff>(WorkspaceDiffParams {
            workspace_id: managed.id.clone(),
            path: None,
            cursor: None,
        })
        .await;

    client
        .typed::<WorkspacesRemove>(RemoveWorkspaceParams {
            workspace_id: managed.id,
            force: true,
            delete_branch: true,
        })
        .await;
    client
        .typed::<WorkspacesForget>(WorkspaceRef {
            workspace_id: adopted.id,
        })
        .await;
    client
        .typed::<ReposRemove>(RepositoryRef {
            repository_id: repository.id,
        })
        .await;
}

// spec: AGT-007
#[tokio::test(flavor = "multi_thread")]
async fn thread_management_methods_return_their_declared_results() {
    let daemon = daemon().await;
    let mut client = daemon.client().await;
    let thread = client.typed::<ThreadsCreate>(new_thread("typed")).await;
    let workspace = client
        .typed::<WorkspacesAdopt>(AdoptWorkspaceParams {
            path: daemon.folder("ws"),
        })
        .await;

    let updated = client
        .typed::<ThreadsUpdate>(UpdateThreadParams {
            thread_id: thread.id.clone(),
            title: Some("renamed".into()),
            tags: Some(vec!["tag".into()]),
        })
        .await;
    assert_eq!(
        (updated.title.as_str(), updated.tags),
        ("renamed", vec!["tag".to_owned()])
    );
    let attached = client
        .typed::<ThreadsAttach>(AttachWorkspaceParams {
            thread_id: thread.id.clone(),
            workspace_id: workspace.id.clone(),
            position: None,
        })
        .await;
    assert_eq!(attached.workspace_ids, std::slice::from_ref(&workspace.id));
    let git = client.typed::<ThreadGit>(thread_ref(&thread.id)).await;
    assert_eq!(git.workspaces.len(), 1);
    let detached = client
        .typed::<ThreadsDetach>(DetachWorkspaceParams {
            thread_id: thread.id.clone(),
            workspace_id: workspace.id,
        })
        .await;
    assert_eq!(detached.workspace_ids, Vec::<String>::new());

    client.typed::<ThreadsArchive>(thread_ref(&thread.id)).await;
    client
        .typed::<ThreadsUnarchive>(thread_ref(&thread.id))
        .await;
    let doomed = client.typed::<ThreadsCreate>(new_thread("doomed")).await;
    client.typed::<ThreadsDelete>(thread_ref(&doomed.id)).await;
}

// spec: AGT-007
#[tokio::test(flavor = "multi_thread")]
async fn agent_methods_the_daemon_answers_itself_return_their_declared_results() {
    let daemon = daemon().await;
    daemon.launcher.push_run(fixture("approval_granted"));
    let mut client = daemon.client().await;
    let thread = client.typed::<ThreadsCreate>(new_thread("agent")).await.id;
    let mut view = Follower::snapshot(&mut client, &thread, None).await;

    let admitted = client
        .typed::<ThreadPrompt>(PromptParams {
            thread_id: thread.clone(),
            text: "run it".into(),
            images: Vec::new(),
            behavior: None,
        })
        .await;
    assert!(admitted.agent_invoked);
    view.until(&mut client, |state| !state.interactions.is_empty())
        .await;
    let Some(ExtensionUiRequest::Select(select)) = view.state.interactions.first().cloned() else {
        panic!(
            "expected the approval select: {:?}",
            view.state.interactions
        );
    };
    client
        .typed::<InteractionRespond>(RespondParams {
            thread_id: thread.clone(),
            interaction_id: select.id,
            response: InteractionResponse::Value(select.options[0].clone()),
        })
        .await;

    client.typed::<ThreadSteer>(message(&thread, "steer")).await;
    client
        .typed::<ThreadFollowUp>(message(&thread, "later"))
        .await;
    client
        .typed::<ThreadSetModel>(SetModelParams {
            thread_id: thread.clone(),
            provider: "fake".into(),
            model_id: "scripted".into(),
        })
        .await;
    client
        .typed::<ThreadSetThinking>(SetThinkingParams {
            thread_id: thread.clone(),
            level: ThinkingLevel::Low,
        })
        .await;
    client
        .typed::<ThreadLabel>(LabelParams {
            thread_id: thread.clone(),
            entry_id: "e1".into(),
            label: Some("label".into()),
        })
        .await;
    let subagent = SubagentRef {
        thread_id: thread.clone(),
        subagent_id: "s1".into(),
    };
    client
        .typed::<ThreadSubagentSteer>(SubagentSteerParams {
            thread_id: thread.clone(),
            subagent_id: subagent.subagent_id.clone(),
            text: "focus".into(),
        })
        .await;
    client.typed::<ThreadSubagentCancel>(subagent).await;
    client.typed::<ThreadRestart>(thread_ref(&thread)).await;
    client.typed::<ThreadStop>(thread_ref(&thread)).await;
}

/// Forwarded methods pass omp's response data through; the fake answers like omp, so these decode as well.
// spec: AGT-007
#[tokio::test(flavor = "multi_thread")]
async fn forwarded_methods_return_their_declared_results_on_the_fake() {
    let daemon = daemon().await;
    let mut client = daemon.client().await;
    let thread = client
        .typed::<ThreadsCreate>(new_thread("forwarded"))
        .await
        .id;
    let queued = |message: &str| QueueMessageParams {
        thread_id: thread.clone(),
        message: message.into(),
        queue: None,
    };

    client.typed::<ThreadState>(thread_ref(&thread)).await;
    client.typed::<ThreadModels>(thread_ref(&thread)).await;
    client
        .typed::<ThreadThinkingLevels>(thread_ref(&thread))
        .await;
    client.typed::<ThreadCommands>(thread_ref(&thread)).await;
    client.typed::<ThreadAbort>(thread_ref(&thread)).await;
    client.typed::<ThreadQueueRemove>(queued("a")).await;
    client.typed::<ThreadQueuePromote>(queued("a")).await;
    client
        .typed::<ThreadCompact>(CompactParams {
            thread_id: thread.clone(),
            instructions: None,
        })
        .await;
    client.typed::<ThreadTree>(thread_ref(&thread)).await;
    client
        .typed::<ThreadNavigate>(NavigateParams {
            thread_id: thread.clone(),
            entry_id: "e1".into(),
            summarize: None,
            instructions: None,
        })
        .await;
    client
        .typed::<ThreadFork>(ForkParams {
            thread_id: thread.clone(),
            entry_id: None,
        })
        .await;
    client.typed::<ThreadSubagents>(thread_ref(&thread)).await;
    client
        .typed::<ThreadSubagentMessages>(SubagentMessagesParams {
            thread_id: thread.clone(),
            subagent_id: "s1".into(),
            from_byte: None,
        })
        .await;
    client
        .typed::<ThreadGoal>(GoalParams {
            thread_id: thread.clone(),
            op: GoalOp::Get,
            objective: None,
            token_budget: None,
        })
        .await;
    client
        .typed::<ThreadBtw>(BtwParams {
            thread_id: thread.clone(),
            question: "why?".into(),
            record_id: None,
        })
        .await;
    client
        .typed::<ThreadBash>(BashParams {
            thread_id: thread.clone(),
            command: "true".into(),
            exclude_from_context: None,
        })
        .await;
}
