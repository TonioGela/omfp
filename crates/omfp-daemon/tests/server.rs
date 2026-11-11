//! Integration tests (L2) of the daemon over a real WebSocket: a daemon on port 0 with a temporary data directory,
//! the fixture-driven fake launcher and fake git, and a native protocol client.

mod client;

use std::path::Path;
use std::sync::Arc;

use clap::Parser;
use client::{Client, Follower};
use omfp_core::ThreadEvent;
use omfp_core::wire::{RpcAgentEvent, RpcNotification};
use omfp_daemon::agent::{FakeLauncher, FakeReply, FakeRun};
use omfp_daemon::config::{Cli, Config};
use omfp_daemon::git::FakeGit;
use omfp_daemon::{DaemonHandle, Ports, discover, start_with};
use omfp_protocol::{ServerFrame, TopicEvent};
use serde_json::{Value, json};
use tempfile::TempDir;

struct TestDaemon {
    handle: DaemonHandle,
    launcher: FakeLauncher,
    git: Arc<FakeGit>,
    dir: TempDir,
}

fn config(dir: &Path, args: &[&str]) -> Config {
    let data_dir = dir.to_str().unwrap();
    let workspaces = dir.join("workspaces");
    let cli = Cli::try_parse_from(
        [
            "daemon",
            "--bind",
            "127.0.0.1:0",
            "--data-dir",
            data_dir,
            "--workspaces-root",
            workspaces.to_str().unwrap(),
        ]
        .into_iter()
        .chain(args.iter().copied()),
    )
    .unwrap();
    Config::from_cli(cli, None, None).unwrap()
}

fn ports(launcher: &FakeLauncher, git: &Arc<FakeGit>) -> Ports {
    Ports {
        launcher: Arc::new(launcher.clone()),
        git: Arc::clone(git) as _,
        omp_program: "omp".into(),
        omp_version: omfp_core::OMP_VERSION.into(),
        omp_env: Vec::new(),
        warnings: vec!["running anyway (--allow-omp-mismatch)".into()],
    }
}

async fn daemon(args: &[&str]) -> TestDaemon {
    let dir = tempfile::tempdir().unwrap();
    let launcher = FakeLauncher::new();
    let git = Arc::new(FakeGit::new());
    let handle = start_with(config(dir.path(), args), ports(&launcher, &git))
        .await
        .unwrap();
    TestDaemon {
        handle,
        launcher,
        git,
        dir,
    }
}

impl TestDaemon {
    async fn client(&self, name: &str) -> Client {
        Client::hello(self.handle.addr(), self.handle.token().expose(), name).await
    }
}

fn fixture(name: &str) -> FakeRun {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/threads")
        .join(omfp_core::OMP_VERSION)
        .join(format!("{name}.jsonl"));
    FakeRun::from_fixture(&path).unwrap()
}

async fn create_thread(client: &mut Client) -> String {
    let thread = client
        .ok("threads.create", json!({ "title": "test thread" }))
        .await;
    thread["id"].as_str().unwrap().to_owned()
}

fn hello(token: &str, protocol: u32) -> String {
    json!({
        "type": "request", "id": "h1", "method": "system.hello",
        "params": { "token": token, "protocol": protocol, "client": "test" }
    })
    .to_string()
}

// spec: PRT-001
#[tokio::test(flavor = "multi_thread")]
async fn any_frame_before_hello_closes_the_connection_with_4401() {
    let daemon = daemon(&[]).await;
    for first in [
        json!({ "type": "request", "id": "1", "method": "repos.suggest" }).to_string(),
        "not json".to_owned(),
    ] {
        let mut client = Client::connect(daemon.handle.addr()).await;
        client.send_text(first).await;
        assert_eq!(client.close_code().await, Some(4401));
    }
}

// spec: PRT-002, PRT-001
#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_token_is_unauthorized_and_closes_the_connection() {
    let daemon = daemon(&[]).await;
    let mut client = Client::connect(daemon.handle.addr()).await;

    client.send_text(hello("not-the-token", 1)).await;

    let response = client.frame().await;
    assert_eq!(response["error"]["code"], "unauthorized");
    assert_eq!(client.close_code().await, Some(4401));
}

/// Whether a browser at `origin` reaching the daemon with `host` upgrades and passes `system.hello`; a refused upgrade
/// is `403`.
async fn upgrades(daemon: &TestDaemon, origin: &str, host: &str) -> bool {
    match Client::connect_with_origin_and_host(daemon.handle.addr(), origin, host).await {
        Ok(mut client) => {
            client
                .send_text(hello(daemon.handle.token().expose(), 1))
                .await;
            assert_eq!(client.frame().await["ok"], true, "{origin}");
            true
        }
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status(), 403, "{origin}");
            false
        }
        Err(error) => panic!("{origin}: {error}"),
    }
}

// spec: PRT-003, PRT-070
#[tokio::test(flavor = "multi_thread")]
async fn without_a_list_only_the_daemons_own_origin_upgrades() {
    let daemon = daemon(&[]).await;
    let own = daemon.handle.addr().to_string();

    assert!(upgrades(&daemon, &format!("http://{own}"), &own).await);
    assert!(!upgrades(&daemon, "https://evil.example", &own).await);
    assert!(
        !upgrades(&daemon, "https://omfp.example.ts.net", &own).await,
        "a proxy that rewrites Host to the daemon's address needs the list"
    );
    assert!(
        upgrades(
            &daemon,
            "https://omfp.example.ts.net",
            "omfp.example.ts.net"
        )
        .await,
        "a proxy that passes the browser's Host on needs no list"
    );
}

// spec: PRT-003, PRT-070, DMN-140
#[tokio::test(flavor = "multi_thread")]
async fn upgrades_from_listed_origins_pass_and_others_are_rejected() {
    let daemon = daemon(&[
        "--allowed-origin",
        "https://omfp.example.ts.net/",
        "--allowed-origin",
        "http://100.64.0.7:7420",
    ])
    .await;
    let own = daemon.handle.addr().to_string();

    for origin in [
        format!("http://{own}"),
        "https://omfp.example.ts.net".to_owned(),
        "http://100.64.0.7:7420".to_owned(),
    ] {
        assert!(upgrades(&daemon, &origin, &own).await, "{origin}");
    }
    for origin in [
        "https://evil.example",
        "http://omfp.example.ts.net",
        "https://omfp.example.ts.net:8443",
        "https://x.omfp.example.ts.net",
        "http://100.64.0.7:7421",
        "null",
    ] {
        assert!(!upgrades(&daemon, origin, &own).await, "{origin}");
    }
}

// spec: PRT-005, PRT-004, PRT-032, PRT-042, PRT-051
#[tokio::test(flavor = "multi_thread")]
async fn hello_reports_versions_and_warnings_and_refuses_unknown_protocols() {
    let daemon = daemon(&[]).await;
    let token = daemon.handle.token().expose().to_owned();

    let mut client = Client::connect(daemon.handle.addr()).await;
    client.send_text(hello(&token, 1)).await;
    let accepted = client.frame().await;
    let base = accepted["result"]["widgetBase"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let key = base.strip_prefix("/widgets/").unwrap_or_default();
    assert!(
        key.len() == 32 && key.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "a 128-bit key: {base}"
    );
    assert_eq!(
        accepted["result"],
        json!({
            "protocol": 1,
            "daemon": env!("CARGO_PKG_VERSION"),
            "omp": omfp_core::OMP_VERSION,
            "warnings": ["running anyway (--allow-omp-mismatch)"],
            "widgetBase": base,
            "connection": accepted["result"]["connection"].as_u64().expect("a connection number"),
        })
    );

    let mut future = Client::connect(daemon.handle.addr()).await;
    future.send_text(hello(&token, 2)).await;
    let refused = future.frame().await;
    assert_eq!(refused["error"]["code"], "version_mismatch");
    assert_eq!(refused["error"]["details"]["supported"], json!([1]));
    assert_eq!(future.close_code().await, Some(4426));
}

// spec: DMN-004, DMN-005
#[tokio::test(flavor = "multi_thread")]
async fn the_library_daemon_serves_until_shutdown_then_stops_omp_and_closes_clients() {
    let daemon = daemon(&[]).await;
    let addr = daemon.handle.addr();
    assert!(
        daemon
            .handle
            .url()
            .ends_with(&format!("{addr}/#token={}", daemon.handle.token().expose()))
    );
    let mut client = daemon.client("test").await;
    let thread = create_thread(&mut client).await;
    client
        .ok("thread.state", json!({ "threadId": thread }))
        .await;
    let process = daemon.launcher.process(0);

    let TestDaemon {
        handle,
        launcher: _launcher,
        dir,
        ..
    } = daemon;
    handle.shutdown().await.unwrap();

    assert!(process.is_closed(), "omp was stopped");
    assert_eq!(client.close_code().await, Some(1001));
    assert!(!dir.path().join("daemon.json").exists());
    assert!(tokio::net::TcpStream::connect(addr).await.is_err());
}

// spec: DMN-007
#[tokio::test(flavor = "multi_thread")]
async fn one_daemon_per_data_directory_published_in_daemon_json() {
    let first = daemon(&[]).await;
    let data_dir = first.dir.path().to_owned();

    let info = discover(&data_dir).unwrap().expect("a running daemon");
    assert_eq!(info.addr, first.handle.addr());
    assert_eq!(info.pid, std::process::id());
    let written: Value =
        serde_json::from_str(&std::fs::read_to_string(data_dir.join("daemon.json")).unwrap())
            .unwrap();
    assert_eq!(written["addr"], first.handle.addr().to_string());

    let fresh = || ports(&FakeLauncher::new(), &Arc::new(FakeGit::new()));
    let second = start_with(config(&data_dir, &[]), fresh()).await;
    let error = second.unwrap_err().to_string();
    assert!(error.contains("already running"), "{error}");
    assert!(error.contains(&first.handle.addr().to_string()), "{error}");

    first.handle.shutdown().await.unwrap();
    assert_eq!(discover(&data_dir).unwrap(), None);
    // The directory is free again.
    let again = start_with(config(&data_dir, &[]), fresh()).await.unwrap();
    again.shutdown().await.unwrap();
}

// spec: DMN-007
#[test]
fn a_stale_daemon_json_is_not_discovered() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("daemon.json"),
        r#"{"addr":"127.0.0.1:9","pid":1}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("daemon.lock"), "").unwrap();

    assert_eq!(discover(dir.path()).unwrap(), None);
    assert_eq!(discover(&dir.path().join("missing")).unwrap(), None);
}

// spec: ARC-004, ARC-005, ARC-001, ARC-011
#[tokio::test(flavor = "multi_thread")]
async fn two_clients_drive_one_omp_process_and_see_the_same_thread() {
    let daemon = daemon(&[]).await;
    daemon.launcher.push_run(fixture("text_reply"));
    let mut web = daemon.client("omfp/web").await;
    let mut desktop = daemon.client("omfp/desktop").await;
    let thread = create_thread(&mut web).await;
    let mut web_view = Follower::snapshot(&mut web, &thread, None).await;
    let mut desktop_view = Follower::snapshot(&mut desktop, &thread, None).await;

    let (first, second) = tokio::join!(
        web.call(
            "thread.prompt",
            json!({ "threadId": thread, "text": "say hi" })
        ),
        desktop.call("thread.state", json!({ "threadId": thread }))
    );
    first.unwrap();
    second.unwrap();
    let done = |state: &omfp_core::ThreadState| state.settled && !state.prompts.is_empty();
    web_view.until(&mut web, done).await;
    desktop_view.until(&mut desktop, done).await;

    // Only the daemon launched omp, once (ARC-001, ARC-005).
    assert_eq!(daemon.launcher.launches().len(), 1);
    assert_eq!(web_view.state, desktop_view.state);
    assert_eq!(
        (&web_view.epoch, web_view.seq),
        (&desktop_view.epoch, desktop_view.seq)
    );
    let latest = Follower::snapshot(&mut web, &thread, None).await;
    assert_eq!(latest.state, web_view.state);
    // Thread events carry omp's frames as `omp_rpc::wire` types (ARC-011).
    let frame = json!({
        "type": "event", "topic": format!("thread/{thread}"), "epoch": "e", "seq": 1,
        "event": { "kind": "omp", "frame": { "type": "session_settled" } }
    });
    let ServerFrame::Event(event) = serde_json::from_value(frame).unwrap() else {
        panic!("not an event");
    };
    let TopicEvent::Thread(ThreadEvent::Omp { frame }) = event.event else {
        panic!("not an omp frame");
    };
    let wire: &omp_rpc::RpcNotification = &frame;
    assert!(matches!(wire, omp_rpc::RpcNotification::SessionSettled(_)));
}

// spec: PRT-020, PRT-021, PRT-022
#[tokio::test(flavor = "multi_thread")]
async fn a_client_reconnecting_with_since_gets_the_missed_events_in_order() {
    let daemon = daemon(&[]).await;
    daemon.launcher.push_run(fixture("bash_tool_call"));
    let mut first = daemon.client("first").await;
    let thread = create_thread(&mut first).await;
    let mut view = Follower::snapshot(&mut first, &thread, None).await;
    first
        .ok(
            "thread.prompt",
            json!({ "threadId": thread, "text": "run echo" }),
        )
        .await;
    // Disconnect mid-stream.
    view.until(&mut first, |state| !state.tools.is_empty())
        .await;
    drop(first);

    let mut observer = daemon.client("observer").await;
    let mut latest = Follower::snapshot(&mut observer, &thread, None).await;
    latest
        .until(&mut observer, |state| {
            state.settled && !state.prompts.is_empty()
        })
        .await;

    let mut second = daemon.client("second").await;
    let result = second
        .ok(
            "subscribe",
            json!({ "topic": format!("thread/{thread}"), "since": { "epoch": view.epoch, "seq": view.seq } }),
        )
        .await;
    assert_eq!(
        result,
        json!({ "mode": "replay", "epoch": view.epoch, "fromSeq": view.seq + 1 })
    );
    while view.seq < latest.seq {
        let frame = second.event(&view.topic).await;
        view.apply(&frame);
    }
    assert_eq!(view.state, latest.state);
}

// spec: PRT-020, PRT-021
#[tokio::test(flavor = "multi_thread")]
async fn an_evicted_position_gets_a_snapshot_equal_to_the_daemon_state() {
    let daemon = daemon(&["--event-buffer", "4"]).await;
    daemon.launcher.push_run(fixture("text_reply"));
    let mut client = daemon.client("test").await;
    let thread = create_thread(&mut client).await;
    let mut view = Follower::snapshot(&mut client, &thread, None).await;
    client
        .ok(
            "thread.prompt",
            json!({ "threadId": thread, "text": "say hi" }),
        )
        .await;
    view.until(&mut client, |state| {
        state.settled && !state.prompts.is_empty()
    })
    .await;

    let mut late = daemon.client("late").await;
    let snapshot = Follower::snapshot(&mut late, &thread, Some((&view.epoch, 1))).await;

    assert_eq!((&snapshot.epoch, snapshot.seq), (&view.epoch, view.seq));
    assert_eq!(snapshot.state, view.state);
}

// spec: PRT-024
#[tokio::test(flavor = "multi_thread")]
async fn unsubscribe_stops_delivery_of_the_topic() {
    let daemon = daemon(&[]).await;
    daemon.launcher.push_run(fixture("text_reply"));
    let mut client = daemon.client("test").await;
    let mut observer = daemon.client("observer").await;
    let thread = create_thread(&mut client).await;
    let topic = format!("thread/{thread}");
    client.ok("subscribe", json!({ "topic": topic })).await;
    let mut latest = Follower::snapshot(&mut observer, &thread, None).await;

    client.ok("unsubscribe", json!({ "topic": topic })).await;
    client
        .ok(
            "thread.prompt",
            json!({ "threadId": thread, "text": "say hi" }),
        )
        .await;
    latest
        .until(&mut observer, |state| {
            state.settled && !state.prompts.is_empty()
        })
        .await;
    client.ok("repos.suggest", json!({})).await;

    let leaked: Vec<Value> = client
        .take_buffered()
        .into_iter()
        .filter(|frame| frame["topic"] == topic)
        .collect();
    assert_eq!(leaked, Vec::<Value>::new());
}

// spec: DMN-071, PRT-011
#[tokio::test(flavor = "multi_thread")]
async fn omp_errors_reach_the_client_with_omps_code() {
    let daemon = daemon(&[]).await;
    daemon.launcher.reply(
        "get_tree",
        FakeReply::Fail {
            message: "session is busy".into(),
            code: Some("session_busy".into()),
        },
    );
    let mut client = daemon.client("test").await;
    let thread = create_thread(&mut client).await;

    let error = client
        .call("thread.tree", json!({ "threadId": thread }))
        .await
        .unwrap_err();

    assert_eq!(error["code"], "omp_error");
    assert_eq!(error["ompCode"], "session_busy");
    let missing = client
        .call("thread.tree", json!({ "threadId": "01NOPE" }))
        .await
        .unwrap_err();
    assert_eq!(missing["code"], "not_found");
}

// spec: PRT-033, ARC-009
#[tokio::test(flavor = "multi_thread")]
async fn created_entities_record_the_creating_client() {
    let daemon = daemon(&[]).await;
    let mut script = daemon.client("script/nightly").await;
    let repo = daemon.external_repo("existing");

    let workspace = script
        .ok("workspaces.create", json!({ "link": repo }))
        .await;
    let thread = script
        .ok(
            "threads.create",
            json!({ "title": "from a script", "workspace": { "id": workspace["id"] } }),
        )
        .await;

    let expected = json!({ "createdBy": { "kind": "user" }, "client": "script/nightly" });
    assert_eq!(workspace["provenance"], expected);
    assert_eq!(thread["provenance"], expected);
    let mut ui = daemon.client("omfp/web").await;
    let index = ui.ok("subscribe", json!({ "topic": "index" })).await;
    assert_eq!(index["state"]["threads"][0]["provenance"], expected);
    assert_eq!(index["state"]["workspaces"][0]["provenance"], expected);
}

/// Params with every field any method needs; methods ignore the others (PRT-012).
fn superset_params(thread: &str, workspace: &str) -> Value {
    json!({
        "threadId": thread, "workspaceId": workspace, "repo": "01NOREPO", "query": "hi",
        "path": "/nonexistent", "pr": "owner/repo#1", "title": "t", "text": "hello", "message": "hello",
        "name": "n", "provider": "fake", "modelId": "scripted", "level": "low", "entryId": "e1",
        "subagentId": "s1", "op": "get", "question": "why?", "command": "true", "interactionId": "i1",
        "response": { "cancelled": true }, "kind": "shell", "cols": 80, "rows": 24, "terminalId": "x",
        "topic": "index"
    })
}

// spec: ARC-007, PRT-033
#[tokio::test(flavor = "multi_thread")]
async fn every_method_is_served_to_a_script_client() {
    let daemon = daemon(&[]).await;
    let mut script = daemon.client("script/smoke").await;
    let thread = script
        .ok("threads.create", json!({ "title": "smoke" }))
        .await;
    let params = superset_params(
        thread["id"].as_str().unwrap(),
        thread["workspaceId"].as_str().unwrap(),
    );

    let mut failures = Vec::new();
    for method in omfp_protocol::METHODS {
        if *method == "system.hello" {
            continue;
        }
        let response = script.request(method, params.clone()).await;
        let error = &response["error"];
        let unknown = error["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("unknown method"));
        if unknown || error["code"] == "internal" {
            failures.push(format!("{method}: {error}"));
        }
    }

    assert_eq!(failures, Vec::<String>::new());
}

impl TestDaemon {
    /// A fake repository outside every workspace.
    fn external_repo(&self, name: &str) -> std::path::PathBuf {
        let path = self.dir.path().join("src").join(name);
        std::fs::create_dir_all(&path).unwrap();
        self.git.add_repo(&path, "main");
        std::fs::canonicalize(path).unwrap()
    }

    /// Local commands (`/add-dir`, `/remove-dir`) the thread's first process got.
    fn local_commands(&self) -> Vec<String> {
        self.launcher
            .process(0)
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                omfp_daemon::agent::FakeCall::Prompt(prompt) if prompt.message.starts_with('/') => {
                    Some(prompt.message)
                }
                _ => None,
            })
            .collect()
    }
}

/// The next index event upserting workspace `id`.
async fn next_workspace(client: &mut Client, id: &Value) -> Value {
    loop {
        let event = client.event("index").await;
        if event["event"]["kind"] == "workspace_upserted"
            && event["event"]["workspace"]["id"] == *id
        {
            return event["event"]["workspace"].clone();
        }
    }
}

// spec: DMN-020, DMN-024, DMN-025, DMN-029, ARC-008
#[tokio::test(flavor = "multi_thread")]
async fn linking_a_repository_into_a_live_threads_workspace_adds_its_root_and_unlinking_removes_it()
{
    let daemon = daemon(&[]).await;
    let mut client = daemon.client("test").await;
    let thread = client
        .ok("threads.create", json!({ "title": "Checkout v2" }))
        .await;
    let id = thread["id"].clone();
    let workspace_id = thread["workspaceId"].clone();
    client.ok("thread.state", json!({ "threadId": id })).await;
    let target = daemon.external_repo("shop-frontend");

    let linked = client
        .ok(
            "workspaces.add_repo",
            json!({ "workspaceId": workspace_id, "source": { "kind": "link", "path": target } }),
        )
        .await;
    assert_eq!(linked["name"], "Checkout v2");
    assert!(
        linked["path"]
            .as_str()
            .unwrap()
            .ends_with("/workspaces/checkout-v2"),
        "{linked}"
    );
    assert_eq!(linked["threadIds"], json!([id]));
    let repo = &linked["repos"][0];
    assert_eq!(
        (&repo["name"], &repo["kind"], &repo["target"]),
        (&json!("shop-frontend"), &json!("link"), &json!(target))
    );
    let link = repo["path"].as_str().unwrap().to_owned();
    client
        .ok(
            "workspaces.remove_repo",
            json!({ "workspaceId": workspace_id, "name": "shop-frontend" }),
        )
        .await;

    assert_eq!(
        daemon.local_commands(),
        [format!("/add-dir {link}"), format!("/remove-dir {link}")]
    );
    assert!(target.is_dir(), "the link's target is never touched");
    let removal = client
        .call(
            "workspaces.remove",
            json!({ "workspaceId": workspace_id, "force": true }),
        )
        .await
        .unwrap_err();
    assert_eq!(
        removal["code"], "conflict",
        "a thread lives there (DMN-028)"
    );
}

// spec: DMN-023, DMN-029, DMN-022
#[tokio::test(flavor = "multi_thread")]
async fn a_repository_the_agent_links_appears_after_its_tool_run() {
    let daemon = daemon(&[]).await;
    let mut client = daemon.client("test").await;
    let thread = client
        .ok("threads.create", json!({ "title": "agent work" }))
        .await;
    let (id, workspace_id) = (thread["id"].clone(), thread["workspaceId"].clone());
    client.ok("thread.state", json!({ "threadId": id })).await;
    client.ok("subscribe", json!({ "topic": "index" })).await;
    let folder = client
        .ok("workspaces.rescan", json!({ "workspaceId": workspace_id }))
        .await["path"]
        .as_str()
        .unwrap()
        .to_owned();
    let link = Path::new(&folder).join("lib");
    std::os::unix::fs::symlink(daemon.external_repo("lib"), &link).unwrap();
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/threads")
        .join(omfp_core::OMP_VERSION)
        .join("bash_tool_call.jsonl");
    let tool_end = std::fs::read_to_string(fixture_path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<ThreadEvent>(line).unwrap())
        .find_map(|event| match event {
            ThreadEvent::Omp {
                frame: frame @ RpcNotification::RpcAgentEvent(RpcAgentEvent::ToolExecutionEnd(_)),
            } => Some(frame),
            _ => None,
        })
        .unwrap();

    daemon.launcher.process(0).emit(vec![tool_end]);

    let workspace = next_workspace(&mut client, &workspace_id).await;
    assert_eq!(workspace["repos"][0]["name"], "lib");
    assert_eq!(workspace["repos"][0]["path"], json!(link));
    while daemon.local_commands().is_empty() {
        tokio::task::yield_now().await;
    }
    assert_eq!(
        daemon.local_commands(),
        [format!("/add-dir {}", link.display())]
    );
}

// spec: DMN-020, DMN-023, DMN-028
#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_open_in_a_window_is_polled_and_removing_it_unlinks_its_repositories() {
    let daemon = daemon(&[]).await;
    let mut client = daemon.client("test").await;
    let app = daemon.external_repo("app");
    let workspace = client.ok("workspaces.create", json!({ "link": app })).await;
    assert_eq!(workspace["name"], "app");
    assert_eq!(
        (
            &workspace["repos"][0]["name"],
            &workspace["repos"][0]["kind"]
        ),
        (&json!("app"), &json!("link"))
    );
    let id = workspace["id"].clone();
    let folder = std::path::PathBuf::from(workspace["path"].as_str().unwrap());
    client.ok("subscribe", json!({ "topic": "index" })).await;
    let topic = format!("workspace/{}", id.as_str().unwrap());
    client.ok("subscribe", json!({ "topic": topic })).await;

    // No request follows: only the poll of the watched workspace sees the new link.
    std::os::unix::fs::symlink(daemon.external_repo("lib"), folder.join("lib")).unwrap();
    let updated = next_workspace(&mut client, &id).await;
    let names: Vec<&str> = updated["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|repo| repo["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["app", "lib"]);

    client
        .ok("workspaces.remove", json!({ "workspaceId": id }))
        .await;
    assert!(!folder.exists());
    assert!(app.is_dir(), "a link's target is never touched");
}

// spec: DMN-026, UI-080
#[tokio::test(flavor = "multi_thread")]
async fn workspaces_status_publishes_the_fresh_status_of_every_repository_on_the_workspace_topic() {
    let daemon = daemon(&[]).await;
    let mut client = daemon.client("test").await;
    let app = daemon.external_repo("app");
    let workspace = client.ok("workspaces.create", json!({ "link": app })).await;
    let id = workspace["id"].as_str().unwrap().to_owned();
    let topic = format!("workspace/{id}");
    let subscribed = client.ok("subscribe", json!({ "topic": topic })).await;
    assert_eq!(
        subscribed["state"][0]["status"]["dirty"], false,
        "{subscribed}"
    );

    // An edit the cheap poll cannot see: `workspaces.status` (what `omfp:changes` calls after a tool run) publishes it.
    daemon.git.set_dirty(&app, true);
    let status = client
        .ok(
            "workspaces.status",
            json!({ "workspaceId": id, "repo": "app" }),
        )
        .await;
    assert_eq!(status["dirty"], true);
    let event = client.event(&topic).await;
    assert_eq!(event["event"]["kind"], "status", "{event}");
    assert_eq!(event["event"]["repos"][0]["name"], "app");
    assert_eq!(event["event"]["repos"][0]["status"]["dirty"], true);
}

// spec: DMN-089, DMN-090
#[tokio::test(flavor = "multi_thread")]
async fn a_written_file_republishes_the_workspace_status_and_a_stale_write_is_a_conflict() {
    let daemon = daemon(&[]).await;
    let mut client = daemon.client("test").await;
    let app = daemon.external_repo("app");
    std::fs::write(app.join("notes.md"), "one\n").unwrap();
    daemon.git.commit_file(&app, "notes.md", b"zero\n");
    let workspace = client.ok("workspaces.create", json!({ "link": app })).await;
    let id = workspace["id"].as_str().unwrap().to_owned();
    let topic = format!("workspace/{id}");
    client.ok("subscribe", json!({ "topic": topic })).await;

    let read = client
        .ok(
            "workspaces.read_file",
            json!({ "workspaceId": id, "path": "notes.md" }),
        )
        .await;
    assert_eq!(read["path"], "app/notes.md");
    assert_eq!(read["kind"], "text");
    assert_eq!(read["content"], "one\n");
    assert_eq!(read["repo"], "app");
    assert_eq!(read["head"], "zero\n");
    let hash = read["hash"].as_str().unwrap().to_owned();

    // What git would now report: the write's rescan publishes it, as after an agent's edit.
    daemon.git.set_dirty(&app, true);
    let written = client
        .ok(
            "workspaces.write_file",
            json!({ "workspaceId": id, "path": "app/notes.md", "content": "two\n", "expectedHash": hash }),
        )
        .await;
    assert_eq!(written["path"], "app/notes.md");
    assert_eq!(written["size"], 4);
    assert_eq!(
        std::fs::read_to_string(app.join("notes.md")).unwrap(),
        "two\n"
    );
    let event = client.event(&topic).await;
    assert_eq!(
        event["event"]["repos"][0]["status"]["dirty"], true,
        "{event}"
    );

    let stale = client
        .call(
            "workspaces.write_file",
            json!({ "workspaceId": id, "path": "app/notes.md", "content": "three\n", "expectedHash": hash }),
        )
        .await
        .unwrap_err();
    assert_eq!(stale["code"], "conflict", "{stale}");
    let exists = client
        .call(
            "workspaces.write_file",
            json!({ "workspaceId": id, "path": "app/notes.md", "content": "three\n" }),
        )
        .await
        .unwrap_err();
    assert_eq!(exists["code"], "conflict", "{exists}");
    assert_eq!(
        std::fs::read_to_string(app.join("notes.md")).unwrap(),
        "two\n"
    );

    let unchanged = client
        .ok(
            "workspaces.read_file",
            json!({ "workspaceId": id, "path": "app/notes.md", "knownHash": written["hash"] }),
        )
        .await;
    assert_eq!(unchanged["unchanged"], true);
    assert_eq!(unchanged.get("content"), None);
    for (path, code) in [
        ("../outside.md", "bad_request"),
        ("app", "bad_request"),
        ("missing.md", "not_found"),
    ] {
        let error = client
            .call(
                "workspaces.read_file",
                json!({ "workspaceId": id, "path": path }),
            )
            .await
            .unwrap_err();
        assert_eq!(error["code"], code, "{path}: {error}");
    }
    let no_folder = client
        .call(
            "workspaces.write_file",
            json!({ "workspaceId": id, "path": "app/docs/new.md", "content": "x" }),
        )
        .await
        .unwrap_err();
    assert_eq!(no_folder["code"], "not_found", "{no_folder}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_index_follows_thread_process_changes() {
    let daemon = daemon(&[]).await;
    let mut client = daemon.client("test").await;
    let thread = create_thread(&mut client).await;
    let index = client.ok("subscribe", json!({ "topic": "index" })).await;
    assert_eq!(index["mode"], "snapshot");
    assert_eq!(index["state"]["threads"][0]["process"]["state"], "stopped");

    client
        .ok("thread.state", json!({ "threadId": thread }))
        .await;

    loop {
        let event = client.event("index").await;
        if event["event"]["kind"] == "thread_upserted"
            && event["event"]["thread"]["process"]["state"] == "running"
        {
            break;
        }
    }
}

// spec: DMN-129
#[tokio::test(flavor = "multi_thread")]
async fn the_thread_list_watch_follows_the_index() {
    let daemon = daemon(&[]).await;
    let mut threads = daemon.handle.threads();
    assert_eq!(threads.borrow_and_update().len(), 0);
    let mut client = daemon.client("test").await;
    let thread = create_thread(&mut client).await;
    until(&mut threads, |list| list.iter().any(|t| t.id == thread)).await;

    client
        .ok("thread.state", json!({ "threadId": thread }))
        .await;
    until(&mut threads, |list| {
        list.iter()
            .any(|t| t.id == thread && t.process == omfp_core::ProcessState::Running)
    })
    .await;

    client
        .ok("threads.delete", json!({ "threadId": thread }))
        .await;
    until(&mut threads, |list| list.iter().all(|t| t.id != thread)).await;
}

/// Waits (at most 10 s) until the thread list satisfies `done`.
async fn until(
    threads: &mut tokio::sync::watch::Receiver<Vec<omfp_protocol::ThreadSummary>>,
    done: impl FnMut(&Vec<omfp_protocol::ThreadSummary>) -> bool,
) {
    tokio::time::timeout(std::time::Duration::from_secs(10), threads.wait_for(done))
        .await
        .unwrap()
        .unwrap();
}

/// The next `thread_upserted` of `thread` on the index.
async fn next_upsert(client: &mut Client, thread: &str) -> Value {
    loop {
        let event = client.event("index").await;
        if event["event"]["kind"] == "thread_upserted" && event["event"]["thread"]["id"] == thread {
            return event["event"]["thread"].clone();
        }
    }
}

// spec: UI-026
#[tokio::test(flavor = "multi_thread")]
async fn archive_unarchive_and_delete_update_the_index() {
    let daemon = daemon(&[]).await;
    let mut client = daemon.client("test").await;
    let thread = create_thread(&mut client).await;
    client.ok("subscribe", json!({ "topic": "index" })).await;

    client
        .ok("threads.archive", json!({ "threadId": thread }))
        .await;
    assert!(next_upsert(&mut client, &thread).await["archivedAt"].is_i64());

    let result = client
        .ok("threads.unarchive", json!({ "threadId": thread }))
        .await;
    assert_eq!(result, json!({}));
    assert_eq!(
        next_upsert(&mut client, &thread).await.get("archivedAt"),
        None
    );

    client
        .ok("threads.delete", json!({ "threadId": thread }))
        .await;
    loop {
        let event = client.event("index").await;
        if event["event"]["kind"] == "thread_removed" {
            assert_eq!(event["event"]["threadId"], thread);
            break;
        }
    }
    let error = client
        .call("threads.unarchive", json!({ "threadId": thread }))
        .await
        .unwrap_err();
    assert_eq!(error["code"], "not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_web_ui_is_served_from_the_static_directory() {
    let site = tempfile::tempdir().unwrap();
    std::fs::write(site.path().join("index.html"), "<main>omfp</main>").unwrap();
    let daemon = daemon(&["--static-dir", site.path().to_str().unwrap()]).await;

    let mut stream = tokio::net::TcpStream::connect(daemon.handle.addr())
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::write_all(
        &mut stream,
        b"GET /threads/x HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await
    .unwrap();
    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("text/html"));
    assert!(response.ends_with("<main>omfp</main>"));
}

/// The raw HTTP/1.1 response to `GET path`, without credentials.
async fn http_get(daemon: &TestDaemon, path: &str) -> String {
    let mut stream = tokio::net::TcpStream::connect(daemon.handle.addr())
        .await
        .unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
        .await
        .unwrap();
    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .unwrap();
    response
}

/// The body of `GET /theme.css`, after checking its headers.
async fn theme_css(daemon: &TestDaemon) -> String {
    let response = http_get(daemon, "/theme.css").await;
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let head = head.to_lowercase();
    assert!(head.starts_with("http/1.1 200"), "{response}");
    assert!(head.contains("content-type: text/css"), "{head}");
    assert!(head.contains("cache-control: no-store"), "{head}");
    body.to_owned()
}

/// The theme of the next `theme_changed` on the index.
async fn next_theme(client: &mut Client) -> Value {
    loop {
        let event = client.event("index").await;
        if event["event"]["kind"] == "theme_changed" {
            return event["event"]["theme"].clone();
        }
    }
}

// spec: DMN-075
#[tokio::test(flavor = "multi_thread")]
async fn a_theme_set_by_one_client_reaches_every_client_and_theme_css() {
    let daemon = daemon(&[]).await;
    let mut web = daemon.client("omfp/web").await;
    let mut desktop = daemon.client("omfp/desktop").await;
    let index = desktop.ok("subscribe", json!({ "topic": "index" })).await;
    assert_eq!(index["state"]["theme"], json!({ "css": null }));
    assert_eq!(theme_css(&daemon).await, "");
    let css = ":root { --omfp-primary: #6750a4; }";

    assert_eq!(web.ok("theme.set", json!({ "css": css })).await, json!({}));

    assert_eq!(next_theme(&mut desktop).await, json!({ "css": css }));
    assert_eq!(web.ok("theme.get", json!({})).await, json!({ "css": css }));
    assert_eq!(theme_css(&daemon).await, css);
    let error = web
        .call(
            "theme.set",
            json!({ "css": "a".repeat(omfp_daemon::theme::MAX_THEME_BYTES + 1) }),
        )
        .await
        .unwrap_err();
    assert_eq!(error["code"], "bad_request");
    assert_eq!(
        theme_css(&daemon).await,
        css,
        "a refused theme changes nothing"
    );

    web.ok("theme.set", json!({ "css": null })).await;
    assert_eq!(next_theme(&mut desktop).await, json!({ "css": null }));
    assert_eq!(theme_css(&daemon).await, "");
}

/// Replaces `file`'s content at once (a new file renamed over it), as editors and theme generators save: a plain
/// `std::fs::write` truncates the file first, so the daemon's poll could read it half-written and push that.
fn replace_file(file: &std::path::Path, content: &str) {
    let next = file.with_extension("next");
    std::fs::write(&next, content).unwrap();
    std::fs::rename(&next, file).unwrap();
}

// spec: DMN-075
#[tokio::test(flavor = "multi_thread")]
async fn the_theme_file_is_read_at_startup_and_its_changes_are_pushed() {
    let config = tempfile::tempdir().unwrap();
    let file = config.path().join("theme.css");
    std::fs::write(&file, "body { color: red; }").unwrap();
    let daemon = daemon(&["--theme-file", file.to_str().unwrap()]).await;
    let mut client = daemon.client("test").await;
    let index = client.ok("subscribe", json!({ "topic": "index" })).await;
    assert_eq!(
        index["state"]["theme"],
        json!({ "css": "body { color: red; }" })
    );

    replace_file(&file, "body { color: blue; }");
    assert_eq!(
        next_theme(&mut client).await,
        json!({ "css": "body { color: blue; }" })
    );
    // The last change wins, from the method or from the file.
    client.ok("theme.set", json!({ "css": "body {}" })).await;
    assert_eq!(next_theme(&mut client).await, json!({ "css": "body {}" }));
    replace_file(&file, "body { color: green; }");
    assert_eq!(
        next_theme(&mut client).await,
        json!({ "css": "body { color: green; }" })
    );
    assert_eq!(theme_css(&daemon).await, "body { color: green; }");
}

// spec: DMN-073
#[tokio::test(flavor = "multi_thread")]
async fn a_thread_created_without_a_title_is_named_from_its_first_message() {
    let daemon = daemon(&[]).await;
    daemon.launcher.push_run(fixture("text_reply"));
    daemon.launcher.push_title(Ok("Greeting the room"));
    let titles = daemon.launcher.gate_titles();
    let mut client = daemon.client("omfp/web").await;
    client.ok("subscribe", json!({ "topic": "index" })).await;

    let thread = client
        .ok(
            "threads.create",
            json!({ "prompt": "say hi to everyone in the room please" }),
        )
        .await;

    assert_eq!(
        (&thread["title"], &thread["titleSource"]),
        (&json!("say hi to everyone in the…"), &json!("placeholder"))
    );
    let id = thread["id"].as_str().unwrap().to_owned();
    let workspace = client
        .ok(
            "workspaces.rescan",
            json!({ "workspaceId": thread["workspaceId"] }),
        )
        .await;
    assert_eq!(workspace["name"], "say hi to everyone in the");
    titles.open();
    loop {
        let summary = next_upsert(&mut client, &id).await;
        if summary["titleSource"] == "generated" {
            assert_eq!(summary["title"], "Greeting the room");
            break;
        }
    }
    let renamed = client
        .ok("threads.update", json!({ "threadId": id, "title": "Mine" }))
        .await;
    assert_eq!(
        (&renamed["title"], &renamed["titleSource"]),
        (&json!("Mine"), &json!("set"))
    );
    let created = client
        .ok(
            "threads.create",
            json!({ "title": "Given", "prompt": "hello" }),
        )
        .await;
    assert_eq!(created["titleSource"], "set");
    assert_eq!(daemon.launcher.title_requests().len(), 1);
}

// spec: DMN-074
#[tokio::test(flavor = "multi_thread")]
async fn new_threads_run_in_yolo_mode_until_another_mode_is_set() {
    let daemon = daemon(&[]).await;
    let mut client = daemon.client("test").await;
    let thread = create_thread(&mut client).await;

    client
        .ok("thread.state", json!({ "threadId": thread }))
        .await;
    client
        .ok(
            "threads.update",
            json!({ "threadId": thread, "approvalMode": "always-ask" }),
        )
        .await;
    client
        .ok("thread.restart", json!({ "threadId": thread }))
        .await;

    let modes: Vec<Option<String>> = daemon
        .launcher
        .launches()
        .into_iter()
        .map(|launch| launch.spec.approval_mode)
        .collect();
    assert_eq!(
        modes,
        [Some("yolo".to_owned()), Some("always-ask".to_owned())]
    );
}

// spec: DMN-029
#[tokio::test(flavor = "multi_thread")]
async fn a_repository_linked_while_omp_starts_is_a_root_before_the_next_prompt() {
    let daemon = daemon(&[]).await;
    let launches = daemon.launcher.gate_launches();
    let mut client = daemon.client("test").await;
    let thread = client
        .ok("threads.create", json!({ "title": "racing" }))
        .await;
    let (id, workspace_id) = (thread["id"].clone(), thread["workspaceId"].clone());
    let index = client.ok("subscribe", json!({ "topic": "index" })).await;
    let folder = index["state"]["workspaces"][0]["path"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut starter = daemon.client("starter").await;
    let starting = tokio::spawn({
        let id = id.clone();
        async move { starter.ok("thread.state", json!({ "threadId": id })).await }
    });
    // omp's flags come from a scan made before the link exists.
    launches.arrived(1).await;
    let link = Path::new(&folder).join("shop");
    std::os::unix::fs::symlink(daemon.external_repo("shop"), &link).unwrap();
    let mut linker = daemon.client("linker").await;
    let rescanning = tokio::spawn({
        let workspace_id = workspace_id.clone();
        async move {
            linker
                .ok("workspaces.rescan", json!({ "workspaceId": workspace_id }))
                .await
        }
    });
    // The rescan publishes the link, then syncs the roots of the thread whose omp still starts; then omp comes up.
    loop {
        let workspace = next_workspace(&mut client, &workspace_id).await;
        if workspace["repos"][0]["path"] == json!(link) {
            break;
        }
    }
    launches.open();
    starting.await.unwrap();
    rescanning.await.unwrap();
    let link = link.display().to_string();

    client
        .ok("thread.prompt", json!({ "threadId": id, "text": "hello" }))
        .await;

    let prompts: Vec<String> = daemon
        .launcher
        .process(0)
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            omfp_daemon::agent::FakeCall::Prompt(prompt) => Some(prompt.message),
            _ => None,
        })
        .collect();
    assert_eq!(prompts, [format!("/add-dir {link}"), "hello".to_owned()]);
}

/// The settings of the next `settings_changed` on the index.
async fn next_settings(client: &mut Client) -> Value {
    loop {
        let event = client.event("index").await;
        if event["event"]["kind"] == "settings_changed" {
            return event["event"]["settings"].clone();
        }
    }
}

// spec: DMN-009, UI-097
#[tokio::test(flavor = "multi_thread")]
async fn settings_come_from_the_config_file_and_settings_set_writes_it_back() {
    let config = tempfile::tempdir().unwrap();
    let file = config.path().join("config.toml");
    std::fs::write(
        &file,
        "# Mine.\n[ui]\ndensity = \"normal\" # roomy\n\n[keymap]\n\"Space t\" = \"theme.set\"\n",
    )
    .unwrap();
    let daemon = daemon(&["--config", file.to_str().unwrap()]).await;
    let mut web = daemon.client("omfp/web").await;
    let mut desktop = daemon.client("omfp/desktop").await;
    let index = desktop.ok("subscribe", json!({ "topic": "index" })).await;
    let expected = json!({
        "ui": { "density": "normal" },
        "keymap": [{ "mode": "normal", "keys": "Space t", "command": "theme.set" }],
        "error": null
    });
    assert_eq!(index["state"]["settings"], expected);
    assert_eq!(web.ok("settings.get", json!({})).await, expected);

    let set = web
        .ok(
            "settings.set",
            json!({
                "ui": { "density": "compact", "fonts": { "code": "Iosevka, monospace" } },
                "keymap": [{ "mode": "insert", "keys": "Ctrl-j", "command": "nav.next" }]
            }),
        )
        .await;

    let expected = json!({
        "ui": { "density": "compact", "fonts": { "code": "Iosevka, monospace" } },
        "keymap": [
            { "mode": "normal", "keys": "Space t", "command": "theme.set" },
            { "mode": "insert", "keys": "Ctrl-j", "command": "nav.next" }
        ],
        "error": null
    });
    assert_eq!(set, expected);
    assert_eq!(next_settings(&mut desktop).await, expected);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.starts_with("# Mine.\n[ui]\ndensity = \"compact\" # roomy\n"),
        "{text}"
    );
    assert!(
        text.contains("[keymap.insert]\nCtrl-j = \"nav.next\""),
        "{text}"
    );
    let error = web
        .call(
            "settings.set",
            json!({ "keymap": [{ "mode": "normal", "keys": "", "command": "x" }] }),
        )
        .await
        .unwrap_err();
    assert_eq!(error["code"], "bad_request");
}

// spec: DMN-009, UI-097
#[tokio::test(flavor = "multi_thread")]
async fn config_file_changes_reach_every_window_and_an_invalid_file_keeps_the_settings() {
    let config = tempfile::tempdir().unwrap();
    let file = config.path().join("config.toml");
    let daemon = daemon(&["--config", file.to_str().unwrap()]).await;
    let mut client = daemon.client("test").await;
    let index = client.ok("subscribe", json!({ "topic": "index" })).await;
    assert_eq!(
        index["state"]["settings"],
        json!({ "ui": {}, "keymap": [], "error": null }),
        "no file yet: the defaults"
    );

    replace_file(&file, "[ui]\nnotifications = \"never\"\n");
    assert_eq!(
        next_settings(&mut client).await,
        json!({ "ui": { "notifications": "never" }, "keymap": [], "error": null })
    );
    replace_file(&file, "[ui]\nnotifications = \"sometimes\"\n");
    let broken = next_settings(&mut client).await;
    assert_eq!(
        broken["ui"],
        json!({ "notifications": "never" }),
        "the previous settings stay"
    );
    let error = broken["error"].as_str().unwrap();
    assert!(
        error.contains("sometimes") && error.contains("config.toml"),
        "{error}"
    );
    // `[daemon]` changes are not applied live (the log asks for a restart); `[ui]` ones are.
    replace_file(
        &file,
        "[daemon]\nmax_processes = 2\n\n[ui]\ndensity = \"normal\"\n",
    );
    assert_eq!(
        next_settings(&mut client).await,
        json!({ "ui": { "density": "normal" }, "keymap": [], "error": null })
    );
}

// spec: DMN-009
#[tokio::test(flavor = "multi_thread")]
async fn without_a_config_file_settings_live_in_memory() {
    let daemon = daemon(&[]).await;
    let mut client = daemon.client("test").await;
    client
        .ok("settings.set", json!({ "ui": { "density": "normal" } }))
        .await;
    let set = client
        .ok(
            "settings.set",
            json!({ "keymap": [{ "mode": "cmdline", "keys": "Ctrl-p", "command": "" }] }),
        )
        .await;
    assert_eq!(
        set,
        json!({
            "ui": { "density": "normal" },
            "keymap": [{ "mode": "cmdline", "keys": "Ctrl-p", "command": "" }],
            "error": null
        })
    );
    assert_eq!(client.ok("settings.get", json!({})).await, set);
}

/// Authenticates as `name` and returns the connection with its number (PRT-051).
async fn numbered(daemon: &TestDaemon, name: &str) -> (Client, u64) {
    let mut client = Client::connect(daemon.handle.addr()).await;
    let hello = client
        .ok(
            "system.hello",
            json!({ "token": daemon.handle.token().expose(), "protocol": 1, "client": name }),
        )
        .await;
    (
        client,
        hello["connection"]
            .as_u64()
            .expect("hello names the connection"),
    )
}

const OFFER: &str = "v=0\r\na=setup:actpass\r\na=mid:0\r\n";

// spec: PRT-051, DMN-100
#[tokio::test(flavor = "multi_thread")]
async fn live_connect_relays_the_offer_and_records_the_owning_connection() {
    let daemon = daemon(&[]).await;
    let (mut first, first_number) = numbered(&daemon, "omfp/web").await;
    let (mut second, second_number) = numbered(&daemon, "omfp/web").await;
    assert_ne!(first_number, second_number);
    let thread = create_thread(&mut first).await;

    let answer = first
        .ok(
            "thread.live_connect",
            json!({ "threadId": thread, "sdpOffer": OFFER }),
        )
        .await;

    assert_eq!(
        answer,
        json!({ "sdpAnswer": omfp_daemon::agent::answer_for(OFFER), "voice": "sol" })
    );
    let mut follower = Follower::snapshot(&mut second, &thread, None).await;
    follower
        .until(&mut second, |state| {
            state.live.as_ref().is_some_and(|live| live.phase.is_some())
        })
        .await;
    let live = follower.state.live.clone().unwrap();
    assert_eq!(live.owner.connection, first_number);
    assert_eq!(live.owner.client, "omfp/web");
}

// spec: DMN-103
#[tokio::test(flavor = "multi_thread")]
async fn closing_the_owning_connection_stops_its_live_session() {
    let daemon = daemon(&[]).await;
    let (mut owner, _) = numbered(&daemon, "omfp/web").await;
    let (mut watcher, _) = numbered(&daemon, "omfp/web").await;
    let thread = create_thread(&mut owner).await;
    owner
        .ok(
            "thread.live_connect",
            json!({ "threadId": thread, "sdpOffer": OFFER }),
        )
        .await;
    let mut follower = Follower::snapshot(&mut watcher, &thread, None).await;

    drop(owner);

    follower
        .until(&mut watcher, |state| state.live.is_none())
        .await;
    let process = daemon.launcher.process(0);
    assert!(!process.live_active());
    assert!(process.calls().iter().any(|call| matches!(
        call,
        omfp_daemon::agent::FakeCall::Call(omfp_daemon::agent::OmpCall::LiveStop)
    )));
}

// spec: DMN-109, PRT-053, DMN-110, PRT-054
#[tokio::test(flavor = "multi_thread")]
async fn live_start_relays_the_voice_and_levels_reach_every_subscriber_as_transient_frames() {
    use omfp_daemon::agent::{FakeLive, live_levels, live_phase};
    let daemon = daemon(&[]).await;
    let (mut desktop, desktop_number) = numbered(&daemon, "omfp/desktop").await;
    let (mut web, _) = numbered(&daemon, "omfp/web").await;
    let thread = create_thread(&mut desktop).await;
    let topic = format!("thread/{thread}");
    let mut follower = Follower::snapshot(&mut web, &thread, None).await;
    daemon.launcher.script_live(FakeLive::connect(vec![
        live_levels(0.25, 0.5),
        live_phase(omp_rpc::LivePhase::Speaking),
    ]));

    let started = desktop
        .ok("thread.live_start", json!({ "threadId": thread }))
        .await;

    assert_eq!(started, json!({ "voice": "sol" }));
    assert_eq!(
        web.transient(&topic).await,
        json!({
            "type": "transient",
            "topic": topic,
            "event": { "kind": "live_levels", "input": 0.25, "output": 0.5 }
        })
    );
    follower
        .until(&mut web, |state| {
            state
                .live
                .as_ref()
                .and_then(|live| live.phase)
                .is_some_and(|phase| phase == omp_rpc::LivePhase::Speaking)
        })
        .await;
    let live = follower.state.live.clone().unwrap();
    assert_eq!(live.owner.connection, desktop_number);
    assert_eq!(live.owner.client, "omfp/desktop");
}

// spec: DMN-109, DMN-103, DMN-101
#[tokio::test(flavor = "multi_thread")]
async fn a_live_start_session_is_the_threads_one_and_stops_with_its_connection() {
    let daemon = daemon(&[]).await;
    let (mut owner, _) = numbered(&daemon, "omfp/desktop").await;
    let (mut watcher, _) = numbered(&daemon, "omfp/web").await;
    let thread = create_thread(&mut owner).await;
    owner
        .ok("thread.live_start", json!({ "threadId": thread }))
        .await;
    let busy = watcher
        .call(
            "thread.live_connect",
            json!({ "threadId": thread, "sdpOffer": OFFER }),
        )
        .await
        .unwrap_err();
    assert_eq!(busy["code"], "conflict", "{busy}");
    let mut follower = Follower::snapshot(&mut watcher, &thread, None).await;

    drop(owner);

    follower
        .until(&mut watcher, |state| state.live.is_none())
        .await;
    assert!(!daemon.launcher.process(0).live_active());
}

// spec: DMN-060
#[tokio::test(flavor = "multi_thread")]
async fn open_file_reaches_the_window_last_used_on_the_thread_and_fails_without_one() {
    let daemon = daemon(&[]).await;
    daemon.launcher.push_run(fixture("text_reply"));
    let (mut web, web_number) = numbered(&daemon, "omfp/web").await;
    let (mut desktop, desktop_number) = numbered(&daemon, "omfp/desktop").await;
    let thread = create_thread(&mut web).await;
    let supervisor = daemon.handle.supervisor();
    assert_eq!(
        supervisor.open_file(&thread, "notes.md", Some(3)),
        Err(omfp_daemon::supervisor::NO_WINDOW.to_owned()),
        "no window watches the thread yet"
    );
    let opened = |path: &'static str| {
        move |state: &omfp_core::ThreadState| {
            state
                .open_file
                .as_ref()
                .is_some_and(|request| request.path == path)
        }
    };

    let mut web_view = Follower::snapshot(&mut web, &thread, None).await;
    let mut desktop_view = Follower::snapshot(&mut desktop, &thread, None).await;
    supervisor.open_file(&thread, "notes.md", Some(3)).unwrap();
    web_view.until(&mut web, opened("notes.md")).await;
    let request = web_view.state.open_file.clone().unwrap();
    assert_eq!(
        (request.connection, request.line),
        (desktop_number, Some(3)),
        "the window that subscribed last"
    );

    web.ok(
        "thread.prompt",
        json!({ "threadId": thread, "text": "say hi" }),
    )
    .await;
    supervisor.open_file(&thread, "src/lib.rs", None).unwrap();
    desktop_view.until(&mut desktop, opened("src/lib.rs")).await;
    assert_eq!(
        desktop_view.state.open_file.clone().unwrap().connection,
        web_number,
        "the window that prompted last"
    );

    for client in [&mut web, &mut desktop] {
        client
            .ok(
                "unsubscribe",
                json!({ "topic": format!("thread/{thread}") }),
            )
            .await;
    }
    assert_eq!(
        supervisor.open_file(&thread, "notes.md", None),
        Err(omfp_daemon::supervisor::NO_WINDOW.to_owned())
    );
}

// spec: DMN-101, DMN-102, DMN-104
#[tokio::test(flavor = "multi_thread")]
async fn live_errors_have_their_codes_and_readable_messages() {
    let daemon = daemon(&[]).await;
    let (mut client, _) = numbered(&daemon, "omfp/web").await;
    let thread = create_thread(&mut client).await;

    let blank = client
        .call(
            "thread.live_connect",
            json!({ "threadId": thread, "sdpOffer": "" }),
        )
        .await
        .unwrap_err();
    assert_eq!(blank["code"], "bad_request", "{blank}");
    let mute = client
        .call(
            "thread.live_mute",
            json!({ "threadId": thread, "muted": true }),
        )
        .await
        .unwrap_err();
    assert_eq!(mute["code"], "conflict", "{mute}");
    assert_eq!(
        client
            .ok("thread.live_stop", json!({ "threadId": thread }))
            .await,
        json!({})
    );

    daemon
        .launcher
        .script_live(omfp_daemon::agent::FakeLive::no_credential());
    let login = client
        .call(
            "thread.live_connect",
            json!({ "threadId": thread, "sdpOffer": OFFER }),
        )
        .await
        .unwrap_err();
    assert_eq!(login["code"], "omp_error", "{login}");
    assert_eq!(
        login["message"],
        "live voice needs an `openai-codex` login in omp: log in with `/login openai-codex` in omp, then try again"
    );
    assert_eq!(
        login["details"]["ompError"],
        omfp_daemon::agent::NO_CREDENTIAL
    );

    daemon
        .launcher
        .script_live(omfp_daemon::agent::FakeLive::default());
    let mut follower = Follower::snapshot(&mut client, &thread, None).await;
    follower
        .until(&mut client, |state| {
            state.live.is_none() && state.live_error.is_some()
        })
        .await;
    client
        .ok(
            "thread.live_connect",
            json!({ "threadId": thread, "sdpOffer": OFFER }),
        )
        .await;
    let second = client
        .call(
            "thread.live_connect",
            json!({ "threadId": thread, "sdpOffer": OFFER }),
        )
        .await
        .unwrap_err();
    assert_eq!(second["code"], "conflict", "{second}");
    assert_eq!(
        client
            .ok(
                "thread.live_mute",
                json!({ "threadId": thread, "muted": true })
            )
            .await,
        json!({ "muted": true })
    );
}
