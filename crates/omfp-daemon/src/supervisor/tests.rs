//! Component tests (L1) of the supervisor: a fixture-driven fake launcher (TST-014), fake git, temp workspace
//! folders, paused time.

use std::path::Path;

use omfp_core::replay;
use omp_rpc::{AgentStartEvent, RpcAgentEvent, RpcNotification};
use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::agent::{FakeCall, FakeLauncher, FakeReply, FakeRun};
use crate::event_log::Start;
use crate::git::{FakeGit, Git};
use crate::host_tools::{OPEN_IN_EDITOR, SET_THREAD_TITLE, run_set_thread_title};
use crate::store::Store;
use crate::threads::NewThread;
use crate::workspaces::{NewWorkspace, Workspaces};

struct Harness {
    dir: TempDir,
    launcher: FakeLauncher,
    supervisor: Supervisor,
    threads: Arc<Threads>,
    workspaces: Arc<Workspaces>,
    git: Arc<FakeGit>,
    data: DataDir,
}

fn harness(configure: impl FnOnce(&mut SupervisorConfig)) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let data = DataDir::new(dir.path().join("data"));
    std::fs::create_dir_all(data.root()).unwrap();
    let root = dir.path().join("workspaces");
    let store = Arc::new(Store::open(&data.db(), &root).unwrap());
    let threads = Arc::new(Threads::new(Arc::clone(&store), data.clone()));
    let git = Arc::new(FakeGit::new());
    let workspaces = Arc::new(Workspaces::new(
        store,
        Arc::clone(&git) as Arc<dyn Git>,
        root,
    ));
    let launcher = FakeLauncher::new();
    let mut config = SupervisorConfig::new("omp");
    configure(&mut config);
    let supervisor = Supervisor::new(
        Arc::new(launcher.clone()),
        Arc::clone(&threads),
        Arc::clone(&workspaces),
        data.clone(),
        config,
    );
    Harness {
        dir,
        launcher,
        supervisor,
        threads,
        workspaces,
        git,
        data,
    }
}

impl Harness {
    /// A thread in a new, empty workspace.
    fn thread(&self) -> String {
        let (ws, _) = self.workspace();
        self.threads.create(NewThread::new("t", ws)).unwrap().id
    }

    /// A new workspace folder: its id and path.
    fn workspace(&self) -> (String, PathBuf) {
        let view = self.workspaces.create(NewWorkspace::new()).unwrap();
        (view.workspace.id, view.workspace.path)
    }

    /// A fake repository outside every workspace.
    fn external_repo(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join("src").join(name);
        std::fs::create_dir_all(&path).unwrap();
        self.git.add_repo(&path, "main");
        path
    }

    /// The local commands (`/add-dir`, `/remove-dir`) the thread's first process got.
    fn local_commands(&self) -> Vec<String> {
        self.launcher
            .process(0)
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                FakeCall::Prompt(prompt) if prompt.message.starts_with('/') => Some(prompt.message),
                _ => None,
            })
            .collect()
    }

    async fn log(&self, id: &str) -> Arc<ThreadLog> {
        self.supervisor.thread_log(id).await.unwrap()
    }

    async fn wait(&self, id: &str, what: &str, done: impl Fn(&ThreadState) -> bool) {
        let log = self.log(id).await;
        let mut live = log.subscribe(None).live;
        while !log.with_state(&done) {
            assert!(
                !matches!(live.recv().await, Err(broadcast::error::RecvError::Closed)),
                "log closed while waiting for {what}"
            );
        }
    }
}

fn fixture_text(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/threads")
        .join(omfp_core::OMP_VERSION)
        .join(format!("{name}.jsonl"));
    std::fs::read_to_string(path).unwrap()
}

fn fixture(name: &str) -> FakeRun {
    FakeRun::from_jsonl(&fixture_text(name)).unwrap()
}

fn prompt(text: &str) -> PromptCommand {
    PromptCommand {
        message: text.into(),
        images: None,
        streaming_behavior: None,
    }
}

fn settled_after_prompt(id: &'static str) -> impl Fn(&ThreadState) -> bool {
    move |state| state.settled && state.prompts.contains_key(id)
}

// spec: TST-014, ARC-012, DMN-070, DMN-040
#[tokio::test]
async fn a_prompt_on_a_stopped_thread_starts_omp_and_reduces_the_recorded_turn() {
    let h = harness(|_| {});
    h.launcher.push_run(fixture("text_reply"));
    let id = h.thread();

    let admission = h.supervisor.prompt(&id, prompt("say hi")).await.unwrap();
    h.wait(&id, "the turn", settled_after_prompt("req_7")).await;

    assert_eq!(admission.prompt_id, "req_7");
    assert_eq!(h.launcher.launches().len(), 1);
    let recorded: Vec<ThreadEvent> = fixture_text("text_reply")
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let log = h.log(&id).await;
    assert_eq!(log.with_state(Clone::clone), replay(&recorded));
    let Start::Snapshot { seq, .. } = log.subscribe(None).start else {
        panic!("no position given");
    };
    // `Starting` belongs to the previous epoch; the new one starts with the history.
    assert_eq!(seq, u64::try_from(recorded.len()).unwrap());
}

// spec: DMN-030, DMN-029, DMN-060, ARC-008, AGT-024, DMN-107
#[tokio::test]
async fn omp_starts_in_its_workspace_folder_with_one_add_dir_per_repository() {
    let agent_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        agent_dir.path().join("APPEND_SYSTEM.md"),
        "Answer in haiku.\n",
    )
    .unwrap();
    let h = harness(|config| {
        config.omp_env = vec![("OMFP_TEST".into(), Some("1".into()))];
        config.agent_dir = Some(agent_dir.path().to_owned());
    });
    let (ws, folder) = h.workspace();
    let external = h.external_repo("shop-frontend");
    std::os::unix::fs::symlink(&external, folder.join("frontend")).unwrap();
    std::fs::create_dir(folder.join("backend")).unwrap();
    h.git.add_repo(&folder.join("backend"), "main");
    std::fs::create_dir(folder.join("notes")).unwrap();
    let mut new = NewThread::new("multi", ws);
    new.model = Some("fake/scripted".into());
    new.approval_mode = Some("always-ask".into());
    let id = h.threads.create(new).unwrap().id;

    h.supervisor.start(&id).await.unwrap();

    let launch = &h.launcher.launches()[0];
    assert_eq!(launch.spec.cwd, folder);
    assert_eq!(
        launch.spec.add_dirs,
        [folder.join("backend"), folder.join("frontend")],
        "repositories by name, the symlink path and not its target; `notes` is no repository"
    );
    assert_eq!(
        launch.spec.append_system_prompt.as_deref(),
        Some(
            format!(
                "{}\n\nAnswer in haiku.",
                conventions::daemon_text("workspace", &folder, &h.data.widgets())
            )
            .as_str()
        ),
        "the conventions, the widgets note (user library), then the user's file"
    );
    assert_eq!(
        launch.spec.session_dir,
        h.threads.get(&id).unwrap().session_dir
    );
    assert_eq!(launch.spec.model.as_deref(), Some("fake/scripted"));
    assert_eq!(launch.spec.approval_mode.as_deref(), Some("always-ask"));
    assert_eq!(launch.spec.stderr_log, h.data.thread_log(&id));
    assert_eq!(
        launch.spec.extension.as_deref(),
        Some(h.data.omp_extension().as_path()),
        "the daemon's omp extension root ships the widget-builder agent"
    );
    assert_eq!(launch.spec.env, [("OMFP_TEST".into(), Some("1".into()))]);
    assert_eq!(launch.tools, [OPEN_IN_EDITOR, SET_THREAD_TITLE]);
    // The session is stored once omp opened it (DMN-032), and its roots are the flags (DMN-029).
    assert!(h.threads.get(&id).unwrap().omp_session_id.is_some());
    assert_eq!(h.threads.roots(&id).unwrap(), launch.spec.add_dirs);
    assert_eq!(h.local_commands(), Vec::<String>::new());
}

// spec: DMN-062, AGT-025
#[tokio::test]
async fn every_launch_registers_the_omfp_host_uri_scheme_next_to_the_tools() {
    let h = harness(|_| {});
    let id = h.thread();

    h.supervisor.start(&id).await.unwrap();
    h.supervisor.stop(&id).await.unwrap();
    h.supervisor.start(&id).await.unwrap();

    let launches = h.launcher.launches();
    assert_eq!(launches.len(), 2);
    for launch in launches {
        assert_eq!(launch.uris, ["omfp"]);
        assert_eq!(launch.tools, [OPEN_IN_EDITOR, SET_THREAD_TITLE]);
    }
}

// spec: DMN-060
#[tokio::test]
async fn host_tool_events_reach_the_thread() {
    let h = harness(|_| {});
    let id = h.thread();
    h.supervisor.start(&id).await.unwrap();
    let log = h.log(&id).await;
    let cwd = h.launcher.launches()[0].spec.cwd.clone();
    let emit: host_tools::Emit = {
        let log = Arc::clone(&log);
        Arc::new(move |event| {
            log.push(event);
        })
    };

    host_tools::run_open_in_editor(
        json!({ "path": "notes.md" }).as_object().unwrap(),
        || false,
        &cwd,
        &*emit,
    )
    .unwrap();

    let url = log.with_state(|state| state.presentation.open_url.clone().unwrap().url);
    assert!(url.ends_with("/workspace/notes.md"), "{url}");
}

// spec: DMN-033
#[tokio::test]
async fn a_session_open_elsewhere_fails_the_start_with_a_clear_error() {
    let h = harness(|_| {});
    let id = h.thread();
    h.threads.set_omp_session_id(&id, "S-busy").unwrap();
    h.launcher.hold_lease("S-busy");

    let error = h.supervisor.start(&id).await.unwrap_err();

    assert!(
        matches!(error, SupervisorError::SessionLeased { .. }),
        "{error}"
    );
    assert!(
        error.to_string().contains("open in another omp process"),
        "{error}"
    );
    assert_eq!(h.launcher.launches().len(), 0);
    let state = h.log(&id).await.with_state(|state| state.process.clone());
    assert!(matches!(state, ProcessState::Failed { message, .. } if message.contains("S-busy")));
    assert!(!h.supervisor.is_running(&id));
}

// spec: DMN-038
#[tokio::test]
async fn starting_beyond_the_process_limit_fails_explicitly() {
    let h = harness(|config| config.max_processes = 1);
    let (a, b) = (h.thread(), h.thread());
    h.supervisor.start(&a).await.unwrap();

    let error = h.supervisor.start(&b).await.unwrap_err();
    assert!(matches!(error, SupervisorError::LimitReached(1)), "{error}");

    h.supervisor.stop(&a).await.unwrap();
    h.supervisor.start(&b).await.unwrap();
    assert!(h.supervisor.is_running(&b));
}

// spec: DMN-035, DMN-032
#[tokio::test]
async fn a_crash_fails_the_thread_and_the_next_prompt_restarts_it_with_history() {
    let h = harness(|_| {});
    h.launcher.push_run(fixture("approval_granted"));
    h.launcher.push_run(fixture("restart_with_history"));
    let id = h.thread();
    h.supervisor.prompt(&id, prompt("run it")).await.unwrap();
    h.wait(&id, "the approval request", |state| {
        !state.interactions.is_empty()
    })
    .await;
    std::fs::write(h.data.thread_log(&id), "booting\nfatal: out of memory\n").unwrap();

    h.launcher.process(0).exit();
    h.wait(&id, "the failure", |state| {
        matches!(state.process, ProcessState::Failed { .. })
    })
    .await;

    let state = h.log(&id).await.with_state(Clone::clone);
    let ProcessState::Failed { stderr_tail, .. } = &state.process else {
        unreachable!()
    };
    assert_eq!(stderr_tail, "booting\nfatal: out of memory");
    assert_eq!(state.interactions, Vec::new());
    assert!(!h.supervisor.is_running(&id));
    assert_eq!(h.launcher.launches().len(), 1, "no automatic restart");

    h.supervisor
        .prompt(&id, prompt("second question"))
        .await
        .unwrap();
    h.wait(&id, "the restarted turn", |state| {
        state.process == ProcessState::Running && state.session.as_ref().is_some_and(|s| s.resumed)
    })
    .await;
    assert_eq!(h.launcher.launches().len(), 2);
    assert!(
        h.log(&id)
            .await
            .with_state(|state| state.transcript.len() >= 2)
    );
}

// spec: DMN-037, ARC-005
#[tokio::test(flavor = "multi_thread")]
async fn concurrent_commands_on_a_stopped_thread_start_a_single_process() {
    let h = harness(|_| {});
    let id = h.thread();

    let calls = (0..8).map(|_| h.supervisor.call(&id, OmpCall::State));
    let results = futures::future::join_all(calls).await;

    assert!(results.iter().all(Result::is_ok));
    assert_eq!(h.launcher.launches().len(), 1);
}

// spec: DMN-037
#[tokio::test(flavor = "multi_thread")]
async fn omp_commands_do_not_wait_for_each_other() {
    let h = harness(|_| {});
    let id = h.thread();
    let gate = h.launcher.gate("compact");
    let supervisor = h.supervisor.clone();
    let compacting = {
        let id = id.clone();
        tokio::spawn(async move {
            supervisor
                .call(
                    &id,
                    OmpCall::Compact(omp_rpc::CompactCommand {
                        custom_instructions: None,
                    }),
                )
                .await
        })
    };
    gate.arrived(1).await;

    // `abort` overtakes the running `compact` (omp orders them itself).
    h.supervisor
        .call(&id, OmpCall::AbortAndRestoreQueue)
        .await
        .unwrap();
    assert!(!compacting.is_finished());

    gate.open();
    compacting.await.unwrap().unwrap();
}

// spec: DMN-071
#[tokio::test]
async fn omp_errors_keep_their_code() {
    let h = harness(|_| {});
    let id = h.thread();
    h.launcher.reply(
        "navigate_tree",
        FakeReply::Fail {
            message: "entry not found".into(),
            code: Some("entry_not_found".into()),
        },
    );

    let error = h
        .supervisor
        .call(
            &id,
            OmpCall::NavigateTree(omp_rpc::NavigateTreeCommand {
                entry_id: "nope".into(),
                summarize: None,
                custom_instructions: None,
            }),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(&error, SupervisorError::Omp { code: Some(code), .. } if code == "entry_not_found"),
        "{error:?}"
    );
}

// spec: DMN-072
#[tokio::test]
async fn a_fork_stores_the_new_session_id() {
    let h = harness(|_| {});
    let id = h.thread();
    h.launcher.reply(
        "get_state",
        FakeReply::Ok(json!({ "sessionId": "S-forked" })),
    );

    h.supervisor
        .call(&id, OmpCall::Fork(omp_rpc::ForkCommand { entry_id: None }))
        .await
        .unwrap();

    assert_eq!(
        h.threads.get(&id).unwrap().omp_session_id.as_deref(),
        Some("S-forked")
    );
    let names: Vec<&str> = h
        .launcher
        .process(0)
        .calls()
        .iter()
        .filter_map(|call| match call {
            FakeCall::Call(call) => Some(call.name()),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["fork", "get_state"]);
}

// spec: DMN-051, DMN-050
#[tokio::test]
async fn the_first_valid_answer_wins_and_later_ones_are_already_resolved() {
    let h = harness(|_| {});
    h.launcher.push_run(fixture("approval_granted"));
    let id = h.thread();
    h.supervisor.prompt(&id, prompt("run it")).await.unwrap();
    h.wait(&id, "the approval request", |state| {
        !state.interactions.is_empty()
    })
    .await;
    let request_id = h
        .log(&id)
        .await
        .with_state(|state| dialog_id(&state.interactions[0]).unwrap().to_owned());

    let invalid = h
        .supervisor
        .respond(
            &id,
            &request_id,
            InteractionResponse::Value("Maybe".into()),
            "web",
        )
        .await
        .unwrap_err();
    assert!(
        matches!(invalid, SupervisorError::InvalidResponse(_)),
        "{invalid}"
    );
    let wrong_kind = h
        .supervisor
        .respond(
            &id,
            &request_id,
            InteractionResponse::Confirmed(true),
            "web",
        )
        .await
        .unwrap_err();
    assert!(matches!(wrong_kind, SupervisorError::InvalidResponse(_)));

    h.supervisor
        .respond(
            &id,
            &request_id,
            InteractionResponse::Value("Approve".into()),
            "web",
        )
        .await
        .unwrap();
    let late = h
        .supervisor
        .respond(&id, &request_id, InteractionResponse::Cancelled, "desktop")
        .await
        .unwrap_err();

    assert!(
        matches!(late, SupervisorError::AlreadyResolved(_)),
        "{late}"
    );
    h.wait(&id, "the end of the turn", settled_after_prompt("req_7"))
        .await;
    let responses: Vec<FakeCall> = h
        .launcher
        .process(0)
        .calls()
        .into_iter()
        .filter(|call| matches!(call, FakeCall::Respond(_)))
        .collect();
    assert_eq!(responses.len(), 1);
}

/// Workspace of `thread`.
fn workspace_of(h: &Harness, thread: &str) -> (String, PathBuf) {
    let ws = h.threads.get(thread).unwrap().workspace_id;
    let path = h.workspaces.get(&ws).unwrap().path;
    (ws, path)
}

// spec: DMN-029, ARC-008
#[tokio::test]
async fn root_sync_adds_new_repositories_and_removes_gone_ones() {
    let h = harness(|_| {});
    let id = h.thread();
    let (ws, folder) = workspace_of(&h, &id);
    h.supervisor.start(&id).await.unwrap();
    let link = folder.join("app");
    std::os::unix::fs::symlink(h.external_repo("app"), &link).unwrap();

    h.workspaces.rescan(&ws).unwrap();
    h.supervisor.sync_roots(&id).await.unwrap();
    assert_eq!(h.threads.roots(&id).unwrap(), std::slice::from_ref(&link));
    std::fs::remove_file(&link).unwrap();
    h.workspaces.rescan(&ws).unwrap();
    h.supervisor.sync_roots(&id).await.unwrap();
    h.supervisor.sync_roots(&id).await.unwrap();

    assert_eq!(
        h.local_commands(),
        [
            format!("/add-dir {}", link.display()),
            format!("/remove-dir {}", link.display())
        ],
        "nothing more once in sync"
    );
    assert_eq!(h.threads.roots(&id).unwrap(), Vec::<PathBuf>::new());
}

// spec: DMN-029
#[tokio::test]
async fn a_busy_thread_syncs_its_roots_once_its_session_settles() {
    let h = harness(|_| {});
    let id = h.thread();
    let (ws, folder) = workspace_of(&h, &id);
    h.supervisor.start(&id).await.unwrap();
    h.launcher.process(0).emit(vec![agent_start()]);
    h.wait(&id, "the busy agent", |state| !state.settled).await;
    let link = folder.join("app");
    std::os::unix::fs::symlink(h.external_repo("app"), &link).unwrap();
    h.workspaces.rescan(&ws).unwrap();

    h.supervisor.sync_roots(&id).await.unwrap();
    assert_eq!(
        h.local_commands(),
        Vec::<String>::new(),
        "omp refuses /add-dir while streaming"
    );
    let settled: RpcNotification =
        serde_json::from_value(json!({ "type": "session_settled" })).unwrap();
    h.launcher.process(0).emit(vec![settled]);
    while h.threads.roots(&id).unwrap().is_empty() {
        tokio::task::yield_now().await;
    }

    assert_eq!(h.local_commands(), [format!("/add-dir {}", link.display())]);
}

// spec: DMN-029
#[tokio::test]
async fn roots_the_session_kept_from_before_are_removed_at_launch() {
    let h = harness(|_| {});
    let id = h.thread();
    let old = h.external_repo("old-worktree");
    h.threads
        .set_roots(&id, std::slice::from_ref(&old))
        .unwrap();

    h.supervisor.start(&id).await.unwrap();

    assert_eq!(
        h.local_commands(),
        [format!("/remove-dir {}", old.display())]
    );
    assert_eq!(h.threads.roots(&id).unwrap(), Vec::<PathBuf>::new());
}

// spec: DMN-023
#[tokio::test]
async fn a_finished_tool_run_is_announced_for_a_rescan() {
    let h = harness(|_| {});
    let id = h.thread();
    let mut tool_ends = h.supervisor.subscribe_tool_ends();
    h.supervisor.start(&id).await.unwrap();
    let end = fixture_text("bash_tool_call")
        .lines()
        .map(|line| serde_json::from_str::<ThreadEvent>(line).unwrap())
        .find_map(|event| match event {
            ThreadEvent::Omp {
                frame: frame @ RpcNotification::RpcAgentEvent(RpcAgentEvent::ToolExecutionEnd(_)),
            } => Some(frame),
            _ => None,
        })
        .unwrap();

    h.launcher.process(0).emit(vec![end]);

    assert_eq!(tool_ends.recv().await.unwrap(), id);
}

fn agent_start() -> RpcNotification {
    let frame = json!({ "type": "agent_start" });
    let event: AgentStartEvent = serde_json::from_value(frame).unwrap();
    RpcNotification::RpcAgentEvent(RpcAgentEvent::AgentStart(event))
}

// spec: DMN-034, PRT-025
#[tokio::test(start_paused = true)]
async fn the_reaper_stops_only_idle_settled_unwatched_threads() {
    let timeout = Duration::from_mins(1);
    let h = harness(|config| config.idle_timeout = timeout);
    let (idle, watched, busy) = (h.thread(), h.thread(), h.thread());
    for id in [&idle, &watched, &busy] {
        h.supervisor.start(id).await.unwrap();
    }
    let guard = h.supervisor.watch(&watched).await.unwrap();
    assert_eq!(h.supervisor.watchers(&watched), 1);
    h.launcher.process(2).emit(vec![agent_start()]);
    h.wait(&busy, "the turn start", |state| !state.settled)
        .await;
    // Paused time jumps to the next timer whenever the runtime waits (here, on the event pump's thread): the reaper
    // starts once the setup is done, so no virtual time passes before it.
    let reaper = h.supervisor.spawn_reaper();

    tokio::time::sleep(timeout / 2).await;
    assert!(h.supervisor.is_running(&idle), "stopped before the timeout");

    h.wait(&idle, "the idle stop", |state| {
        state.process == ProcessState::Stopped
    })
    .await;
    tokio::time::sleep(timeout * 10).await;
    assert!(
        h.supervisor.is_running(&watched),
        "a watched thread is never stopped"
    );
    assert!(
        h.supervisor.is_running(&busy),
        "a busy thread is never stopped"
    );
    assert!(h.launcher.process(0).is_closed());

    drop(guard);
    assert_eq!(h.supervisor.watchers(&watched), 0);
    h.wait(&watched, "the stop once unwatched", |state| {
        state.process == ProcessState::Stopped
    })
    .await;
    reaper.abort();
}

// spec: DMN-005
#[tokio::test]
async fn shutdown_stops_every_process_and_refuses_new_ones() {
    let h = harness(|_| {});
    let (a, b) = (h.thread(), h.thread());
    h.supervisor.start(&a).await.unwrap();
    h.supervisor.start(&b).await.unwrap();

    h.supervisor.shutdown().await;

    for (index, id) in [&a, &b].into_iter().enumerate() {
        assert!(h.launcher.process(index).is_closed());
        let process = h.log(id).await.with_state(|state| state.process.clone());
        assert_eq!(process, ProcessState::Stopped);
    }
    assert!(matches!(
        h.supervisor.start(&a).await,
        Err(SupervisorError::ShuttingDown)
    ));
}

#[test]
fn the_stderr_tail_keeps_the_last_lines() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omp.log");
    let text = (1..=30)
        .map(|line| format!("line {line}\n"))
        .collect::<Vec<_>>()
        .concat();
    std::fs::write(&path, text).unwrap();

    let tail = stderr_tail(&path, 3);

    assert_eq!(tail, "line 28\nline 29\nline 30");
    assert_eq!(stderr_tail(&dir.path().join("missing"), 3), "");
}

impl Harness {
    /// A thread without a title in a new workspace: it gets one from its first message (DMN-073).
    fn untitled_thread(&self) -> (String, PathBuf) {
        let (ws, path) = self.workspace();
        let new = NewThread {
            title_source: TitleSource::Placeholder,
            ..NewThread::new("", ws)
        };
        (self.threads.create(new).unwrap().id, path)
    }

    fn title(&self, id: &str) -> (String, TitleSource) {
        let thread = self.threads.get(id).unwrap();
        (thread.title, thread.title_source)
    }

    /// The sessions names the thread's first process got.
    fn session_names(&self) -> Vec<String> {
        self.launcher
            .process(0)
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                FakeCall::Call(OmpCall::SetSessionName(command)) => Some(command.name),
                _ => None,
            })
            .collect()
    }
}

// spec: DMN-073
#[tokio::test]
async fn the_first_message_gives_a_placeholder_then_a_generated_title_and_session_name() {
    let h = harness(|config| {
        config.omp_env = vec![("OMFP_TEST".into(), Some("1".into()))];
    });
    h.launcher.push_run(fixture("text_reply"));
    h.launcher.push_title(Ok("Greeting the room"));
    let (id, folder) = h.untitled_thread();

    h.supervisor
        .prompt(&id, prompt("say hi to everyone in the room please"))
        .await
        .unwrap();
    assert_eq!(
        h.title(&id),
        (
            "say hi to everyone in the…".into(),
            TitleSource::Placeholder
        )
    );
    h.supervisor.title_generated(&id).await;

    assert_eq!(
        h.title(&id),
        ("Greeting the room".into(), TitleSource::Generated)
    );
    assert_eq!(h.session_names(), ["Greeting the room"]);
    let requests = h.launcher.title_requests();
    assert_eq!(
        requests,
        [TitleRequest {
            program: "omp".into(),
            cwd: folder,
            env: vec![("OMFP_TEST".into(), Some("1".into()))],
            message: "say hi to everyone in the room please".into(),
        }]
    );
    // Later messages keep the title.
    h.supervisor.prompt(&id, prompt("again")).await.unwrap();
    assert_eq!(h.launcher.title_requests().len(), 1);
}

// spec: DMN-073
#[tokio::test]
async fn a_title_set_while_the_title_is_generated_is_kept() {
    let h = harness(|_| {});
    h.launcher.push_run(fixture("text_reply"));
    h.launcher.push_title(Ok("Generated"));
    let gate = h.launcher.gate_titles();
    let (id, _) = h.untitled_thread();
    h.supervisor.prompt(&id, prompt("say hi")).await.unwrap();
    gate.arrived(1).await;

    h.supervisor
        .set_title(&id, "Mine".into(), TitleSource::Set)
        .await
        .unwrap();
    gate.open();
    h.supervisor.title_generated(&id).await;

    assert_eq!(h.title(&id), ("Mine".into(), TitleSource::Set));
    assert_eq!(h.session_names(), ["Mine"]);
}

// spec: DMN-073
#[tokio::test]
async fn a_set_title_is_never_generated_and_a_failed_generation_keeps_the_placeholder() {
    let h = harness(|_| {});
    let titled = h.thread();
    h.supervisor
        .prompt(&titled, prompt("say hi"))
        .await
        .unwrap();
    assert_eq!(h.title(&titled), ("t".into(), TitleSource::Set));
    assert_eq!(h.launcher.title_requests(), Vec::new());

    h.launcher.push_title(Err("no model for the smol role"));
    let (untitled, _) = h.untitled_thread();
    h.supervisor
        .prompt(&untitled, prompt("fix the build"))
        .await
        .unwrap();
    h.supervisor.title_generated(&untitled).await;
    assert_eq!(
        h.title(&untitled),
        ("fix the build".into(), TitleSource::Placeholder)
    );
}

// spec: DMN-073
#[tokio::test]
async fn without_title_generation_the_placeholder_stays() {
    let h = harness(|config| config.title_generation = false);
    let (id, _) = h.untitled_thread();

    h.supervisor
        .prompt(&id, prompt("fix the build"))
        .await
        .unwrap();
    h.supervisor.title_generated(&id).await;

    assert_eq!(
        h.title(&id),
        ("fix the build".into(), TitleSource::Placeholder)
    );
    assert_eq!(h.launcher.title_requests(), Vec::new());
}

// spec: DMN-060, DMN-073
#[tokio::test]
async fn set_thread_title_stores_a_set_title_and_names_the_session() {
    let h = harness(|_| {});
    let (id, _) = h.untitled_thread();
    h.supervisor.start(&id).await.unwrap();
    let mut changes = h.supervisor.subscribe_changes();
    let gate = h.launcher.gate("set_session_name");
    let set_title = h.supervisor.title_setter(&id);

    // omp-rpc runs host tool handlers on their own threads.
    let result = tokio::task::spawn_blocking(move || {
        let arguments = json!({ "title": "Agent's choice" });
        run_set_thread_title(arguments.as_object().unwrap(), || false, &*set_title)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
    .await
    .unwrap();

    assert_eq!(result, Ok(()));
    assert_eq!(h.title(&id), ("Agent's choice".into(), TitleSource::Set));
    assert_eq!(changes.recv().await.unwrap(), id);
    gate.arrived(1).await;
    gate.open();
    assert_eq!(h.session_names(), ["Agent's choice"]);
}

fn owner(connection: u64) -> LiveOwner {
    LiveOwner {
        client: "omfp/web".into(),
        connection,
    }
}

const OFFER: &str = "v=0\r\na=setup:actpass\r\na=mid:0\r\n";

/// The `live_connect` offers a fake process received.
fn live_offers(process: &crate::agent::FakeProcess) -> Vec<String> {
    process
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            FakeCall::Call(OmpCall::LiveConnect(connect)) => Some(connect.sdp_offer),
            _ => None,
        })
        .collect()
}

fn live_phase(state: &ThreadState) -> Option<&'static str> {
    state
        .live
        .as_ref()
        .and_then(|live| live.phase)
        .map(omp_rpc::LivePhase::as_str)
}

// spec: DMN-100, TST-015
#[tokio::test]
async fn live_connect_starts_omp_records_the_owner_and_relays_the_answer() {
    let h = harness(|_| {});
    let id = h.thread();
    h.launcher.script_live(crate::agent::FakeLive::connect(
        crate::agent::FakeLive::call_frames(),
    ));

    let answer = h
        .supervisor
        .live_connect(&id, owner(4), OFFER.into())
        .await
        .unwrap();

    assert_eq!(answer.voice, "sol");
    assert_eq!(answer.sdp_answer, crate::agent::answer_for(OFFER));
    assert_eq!(live_offers(&h.launcher.process(0)), vec![OFFER.to_owned()]);
    h.wait(&id, "the call's transcript", |state| {
        state
            .live
            .as_ref()
            .is_some_and(|live| live.transcript.len() == 2)
    })
    .await;
    let state = h.log(&id).await.with_state(Clone::clone);
    let live = state.live.clone().unwrap();
    assert_eq!(live.owner, owner(4));
    assert_eq!(live_phase(&state), Some("listening"));
    assert!(live.transcript.iter().all(|line| line.r#final));
}

// spec: DMN-105
#[tokio::test]
async fn live_connect_sends_the_live_prompt_with_the_threads_conversation() {
    let h = harness(|_| {});
    let (ws, _) = h.workspace();
    let id = h
        .threads
        .create(NewThread::new("Login bug", ws))
        .unwrap()
        .id;
    let history = [
        json!({ "role": "user", "content": "why does {{login}} fail?" }),
        json!({ "role": "assistant", "content": [{ "type": "text", "text": "The token was stale." }] }),
    ]
    .map(|message| omp_rpc::AgentMessage::from_value(message).unwrap());
    h.launcher
        .push_run(FakeRun::fresh().with_history(history.into()));

    h.supervisor
        .live_connect(&id, owner(1), OFFER.into())
        .await
        .unwrap();

    let instructions: Vec<Option<String>> = h
        .launcher
        .process(0)
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            FakeCall::Call(OmpCall::LiveConnect(connect)) => Some(connect.instructions),
            _ => None,
        })
        .collect();
    let state = h.log(&id).await.with_state(Clone::clone);
    assert_eq!(
        instructions,
        vec![Some(crate::live_context::live_instructions(
            "Login bug",
            &state
        ))]
    );
    let sent = instructions[0].as_deref().unwrap();
    assert!(sent.starts_with("You: omp Live, realtime voice surface"));
    assert!(sent.contains("Thread title: Login bug"));
    assert!(sent.contains(
        "User: why does {{default \"\" \"{{\"}}login}} fail?\n\nAssistant: The token was stale."
    ));
}

// spec: DMN-100
#[tokio::test]
async fn a_blank_offer_is_refused_without_omp() {
    let h = harness(|_| {});
    let id = h.thread();

    let error = h
        .supervisor
        .live_connect(&id, owner(1), " \n".into())
        .await
        .unwrap_err();

    assert!(matches!(error, SupervisorError::EmptyOffer), "{error:?}");
    assert_eq!(h.launcher.launches().len(), 0);
}

// spec: DMN-101, DMN-102
#[tokio::test]
async fn one_live_session_per_thread_until_omp_ends_it() {
    let h = harness(|_| {});
    let (id, other) = (h.thread(), h.thread());
    h.supervisor
        .live_connect(&id, owner(1), OFFER.into())
        .await
        .unwrap();

    let error = h
        .supervisor
        .live_connect(&id, owner(2), OFFER.into())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, SupervisorError::LiveActive { client } if client == "omfp/web"),
        "{error:?}"
    );
    assert_eq!(
        live_offers(&h.launcher.process(0)).len(),
        1,
        "omp never saw the second offer"
    );
    h.supervisor
        .live_connect(&other, owner(2), OFFER.into())
        .await
        .expect("another thread has its own session");

    h.supervisor.live_stop(&id).await.unwrap();
    h.wait(&id, "the end of the session", |state| state.live.is_none())
        .await;
    h.supervisor
        .live_connect(&id, owner(3), OFFER.into())
        .await
        .expect("a new session once omp ended the first");
    let state = h.log(&id).await.with_state(Clone::clone);
    assert_eq!(state.live.map(|live| live.owner), Some(owner(3)));
}

// spec: DMN-102
#[tokio::test]
async fn live_stop_and_mute_never_start_omp() {
    let h = harness(|_| {});
    let id = h.thread();

    h.supervisor.live_stop(&id).await.unwrap();
    let error = h.supervisor.live_mute(&id, true).await.unwrap_err();

    assert!(matches!(error, SupervisorError::NoLiveSession), "{error:?}");
    assert_eq!(h.launcher.launches().len(), 0);
}

// spec: DMN-102, TST-015
#[tokio::test]
async fn live_mute_forwards_the_mute_and_omp_reports_the_phase() {
    let h = harness(|_| {});
    let id = h.thread();
    h.supervisor
        .live_connect(&id, owner(1), OFFER.into())
        .await
        .unwrap();

    assert!(h.supervisor.live_mute(&id, true).await.unwrap());
    h.wait(&id, "the muted phase", |state| {
        live_phase(state) == Some("muted")
    })
    .await;
    assert!(!h.supervisor.live_mute(&id, false).await.unwrap());
    h.wait(&id, "the listening phase", |state| {
        live_phase(state) == Some("listening")
    })
    .await;
    let mutes: Vec<Option<bool>> = h
        .launcher
        .process(0)
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            FakeCall::Call(OmpCall::LiveMute(mute)) => Some(mute.muted),
            _ => None,
        })
        .collect();
    assert_eq!(mutes, vec![Some(true), Some(false)]);
}

// spec: DMN-104, TST-015
#[tokio::test]
async fn a_missing_codex_login_reads_as_a_login_hint() {
    let h = harness(|_| {});
    let id = h.thread();
    h.launcher
        .script_live(crate::agent::FakeLive::no_credential());

    let error = h
        .supervisor
        .live_connect(&id, owner(1), OFFER.into())
        .await
        .unwrap_err();

    let SupervisorError::Live { message, omp } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(omp, crate::agent::NO_CREDENTIAL);
    assert!(message.contains("`openai-codex` login"), "{message}");
    assert!(message.contains("/login openai-codex"), "{message}");
    h.wait(&id, "the end of the session", |state| {
        state.live.is_none() && state.live_error.is_some()
    })
    .await;
    let state = h.log(&id).await.with_state(Clone::clone);
    assert_eq!(state.live_error.as_deref(), Some(message.as_str()));
    // omp's `live_end` freed the thread for another try.
    h.launcher.script_live(crate::agent::FakeLive::default());
    h.supervisor
        .live_connect(&id, owner(1), OFFER.into())
        .await
        .unwrap();
}

// spec: DMN-101, DMN-104
#[tokio::test]
async fn a_refusal_before_the_session_closes_it_at_once() {
    let h = harness(|_| {});
    let id = h.thread();
    h.launcher.reply(
        "live_connect",
        FakeReply::Fail {
            message: "End dictation before starting live voice".into(),
            code: None,
        },
    );

    let error = h
        .supervisor
        .live_connect(&id, owner(1), OFFER.into())
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "end dictation before starting live voice"
    );
    let state = h.log(&id).await.with_state(Clone::clone);
    assert_eq!(state.live, None, "a `live_session` without owner closed it");
    let again = h
        .supervisor
        .live_connect(&id, owner(1), OFFER.into())
        .await
        .unwrap_err();
    assert!(matches!(again, SupervisorError::Live { .. }), "{again:?}");
}

// spec: DMN-103
#[tokio::test]
async fn a_closed_connection_stops_only_its_own_sessions() {
    let h = harness(|_| {});
    let (mine, theirs) = (h.thread(), h.thread());
    h.supervisor
        .live_connect(&mine, owner(1), OFFER.into())
        .await
        .unwrap();
    h.supervisor
        .live_connect(&theirs, owner(2), OFFER.into())
        .await
        .unwrap();

    h.supervisor.release_live(1).await;

    h.wait(&mine, "the stop", |state| state.live.is_none())
        .await;
    assert!(!h.launcher.process(0).live_active());
    assert!(h.launcher.process(1).live_active());
    let state = h.log(&theirs).await.with_state(Clone::clone);
    assert_eq!(state.live.map(|live| live.owner), Some(owner(2)));
}

// spec: DMN-103
#[tokio::test]
async fn a_live_session_ends_with_its_process() {
    let h = harness(|_| {});
    let id = h.thread();
    h.supervisor
        .live_connect(&id, owner(1), OFFER.into())
        .await
        .unwrap();

    h.launcher.process(0).exit();
    h.wait(&id, "the crash", |state| {
        matches!(state.process, ProcessState::Failed { .. })
    })
    .await;

    assert_eq!(
        h.log(&id).await.with_state(|state| state.live.clone()),
        None
    );
    h.supervisor
        .live_connect(&id, owner(1), OFFER.into())
        .await
        .expect("the slot died with the process");
    assert_eq!(h.launcher.launches().len(), 2);
}

// spec: DMN-103, DMN-034
#[tokio::test(start_paused = true)]
async fn the_reaper_keeps_a_process_with_a_live_session() {
    let timeout = Duration::from_mins(1);
    let h = harness(|config| config.idle_timeout = timeout);
    let id = h.thread();
    h.supervisor
        .live_connect(&id, owner(1), OFFER.into())
        .await
        .unwrap();
    let reaper = h.supervisor.spawn_reaper();

    tokio::time::sleep(timeout * 10).await;
    assert!(h.supervisor.is_running(&id), "a live session keeps omp");

    h.supervisor.live_stop(&id).await.unwrap();
    h.wait(&id, "the idle stop after the session", |state| {
        state.process == ProcessState::Stopped
    })
    .await;
    reaper.abort();
}
