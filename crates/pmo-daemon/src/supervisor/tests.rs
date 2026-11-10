//! Component tests (L1) of the supervisor: a fixture-driven fake launcher (TST-014), fake git, paused time.

use std::path::Path;

use omp_rpc::{AgentStartEvent, RpcAgentEvent, RpcNotification};
use pmo_core::replay;
use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::agent::{FakeCall, FakeLauncher, FakeReply, FakeRun};
use crate::event_log::Start;
use crate::git::FakeGit;
use crate::host_tools::OPEN_IN_EDITOR;
use crate::store::{Provenance, Store};
use crate::threads::NewThread;
use crate::workspaces::Workspaces;

struct Harness {
    _dir: TempDir,
    launcher: FakeLauncher,
    supervisor: Supervisor,
    threads: Arc<Threads>,
    workspaces: Arc<Workspaces>,
    data: DataDir,
}

fn harness(configure: impl FnOnce(&mut SupervisorConfig)) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let data = DataDir::new(dir.path());
    let store = Arc::new(Store::open(&data.db()).unwrap());
    let threads = Arc::new(Threads::new(Arc::clone(&store), data.clone()));
    let workspaces = Arc::new(Workspaces::new(
        store,
        Arc::new(FakeGit::new()),
        data.clone(),
    ));
    let launcher = FakeLauncher::new();
    let mut config = SupervisorConfig::new("omp");
    configure(&mut config);
    let supervisor = Supervisor::new(
        Arc::new(launcher.clone()),
        Arc::clone(&threads),
        data.clone(),
        config,
    );
    Harness {
        _dir: dir,
        launcher,
        supervisor,
        threads,
        workspaces,
        data,
    }
}

impl Harness {
    fn thread(&self) -> String {
        self.threads.create(NewThread::new("t")).unwrap().id
    }

    fn workspace(&self, name: &str) -> (String, PathBuf) {
        let path = self.data.root().join(name);
        std::fs::create_dir_all(&path).unwrap();
        let ws = self.workspaces.adopt(&path, Provenance::User).unwrap();
        (ws.id, ws.path)
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
        .join(pmo_core::OMP_VERSION)
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

// spec: DMN-030, DMN-028, DMN-060
#[tokio::test]
async fn omp_starts_in_the_thread_home_with_its_workspaces_and_host_tools() {
    let h = harness(|config| config.omp_env = vec![("PMO_TEST".into(), Some("1".into()))]);
    let (first, first_path) = h.workspace("frontend");
    let (second, second_path) = h.workspace("backend");
    let mut new = NewThread::new("multi");
    new.workspace_ids = vec![first, second];
    new.model = Some("fake/scripted".into());
    new.approval_mode = Some("always-ask".into());
    let id = h.threads.create(new).unwrap().id;

    h.supervisor.start(&id).await.unwrap();

    let launch = &h.launcher.launches()[0];
    assert_eq!(
        launch.spec.cwd,
        std::path::absolute(h.data.thread_home(&id)).unwrap()
    );
    assert_eq!(launch.spec.add_dirs, [first_path, second_path]);
    assert_eq!(
        launch.spec.session_dir,
        h.threads.get(&id).unwrap().session_dir
    );
    assert_eq!(launch.spec.model.as_deref(), Some("fake/scripted"));
    assert_eq!(launch.spec.approval_mode.as_deref(), Some("always-ask"));
    assert_eq!(launch.spec.stderr_log, h.data.thread_log(&id));
    assert_eq!(launch.spec.env, [("PMO_TEST".into(), Some("1".into()))]);
    assert_eq!(launch.tools, [OPEN_IN_EDITOR]);
    // The session is stored once omp opened it (DMN-032).
    assert!(h.threads.get(&id).unwrap().omp_session_id.is_some());
}

// spec: DMN-060
#[tokio::test]
async fn host_tool_events_reach_the_thread() {
    let h = harness(|_| {});
    let id = h.thread();
    h.supervisor.start(&id).await.unwrap();
    let log = h.log(&id).await;
    let home = h.data.thread_home(&id);
    let emit: host_tools::Emit = {
        let log = Arc::clone(&log);
        Arc::new(move |event| {
            log.push(event);
        })
    };

    host_tools::run_open_in_editor(
        json!({ "path": "notes.md" }).as_object().unwrap(),
        || false,
        &home,
        &*emit,
    )
    .unwrap();

    let url = log.with_state(|state| state.presentation.open_url.clone().unwrap().url);
    assert!(url.ends_with("/home/notes.md"), "{url}");
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

// spec: DMN-029
#[tokio::test]
async fn attaching_to_a_running_thread_sends_add_dir_and_detaching_remove_dir() {
    let h = harness(|_| {});
    let id = h.thread();
    let (stopped_ws, _) = h.workspace("early");
    h.supervisor.attach(&id, &stopped_ws, None).await.unwrap();
    h.supervisor.start(&id).await.unwrap();
    let (ws, path) = h.workspace("late");

    let thread = h.supervisor.attach(&id, &ws, None).await.unwrap();
    h.supervisor.detach(&id, &ws).await.unwrap();

    assert_eq!(thread.workspace_ids, [stopped_ws, ws]);
    let prompts: Vec<String> = h
        .launcher
        .process(0)
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            FakeCall::Prompt(prompt) => Some(prompt.message),
            _ => None,
        })
        .collect();
    assert_eq!(
        prompts,
        [
            format!("/add-dir {}", path.display()),
            format!("/remove-dir {}", path.display())
        ]
    );
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
