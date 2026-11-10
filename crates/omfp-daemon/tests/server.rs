//! Integration tests (L2) of the daemon over a real WebSocket: a daemon on port 0 with a temporary data directory,
//! the fixture-driven fake launcher and fake git, and a native protocol client.

mod client;

use std::path::Path;
use std::sync::Arc;

use clap::Parser;
use client::{Client, Follower};
use omfp_core::ThreadEvent;
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
    dir: TempDir,
}

fn config(dir: &Path, args: &[&str]) -> Config {
    let data_dir = dir.to_str().unwrap();
    let cli = Cli::try_parse_from(
        ["daemon", "--bind", "127.0.0.1:0", "--data-dir", data_dir]
            .into_iter()
            .chain(args.iter().copied()),
    )
    .unwrap();
    Config::from_cli(cli, None, None).unwrap()
}

fn ports(launcher: &FakeLauncher) -> Ports {
    Ports {
        launcher: Arc::new(launcher.clone()),
        git: Arc::new(FakeGit::new()),
        omp_program: "omp".into(),
        omp_version: omfp_core::OMP_VERSION.into(),
        omp_env: Vec::new(),
        warnings: vec!["running anyway (--allow-omp-mismatch)".into()],
    }
}

async fn daemon(args: &[&str]) -> TestDaemon {
    let dir = tempfile::tempdir().unwrap();
    let launcher = FakeLauncher::new();
    let handle = start_with(config(dir.path(), args), ports(&launcher))
        .await
        .unwrap();
    TestDaemon {
        handle,
        launcher,
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
        json!({ "type": "request", "id": "1", "method": "repos.list" }).to_string(),
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

// spec: PRT-003
#[tokio::test(flavor = "multi_thread")]
async fn upgrades_from_foreign_origins_are_rejected() {
    let daemon = daemon(&["--allow-origin", "https://omfp.example.com"]).await;
    let addr = daemon.handle.addr();

    assert!(
        Client::connect_with_origin(addr, "https://evil.example")
            .await
            .is_err()
    );
    for origin in [
        format!("http://{addr}"),
        "https://omfp.example.com".to_owned(),
    ] {
        let mut client = Client::connect_with_origin(addr, &origin).await.unwrap();
        client
            .send_text(hello(daemon.handle.token().expose(), 1))
            .await;
        assert_eq!(client.frame().await["ok"], true, "{origin}");
    }
}

// spec: PRT-005, PRT-004, PRT-032
#[tokio::test(flavor = "multi_thread")]
async fn hello_reports_versions_and_warnings_and_refuses_unknown_protocols() {
    let daemon = daemon(&[]).await;
    let token = daemon.handle.token().expose().to_owned();

    let mut client = Client::connect(daemon.handle.addr()).await;
    client.send_text(hello(&token, 1)).await;
    let accepted = client.frame().await;
    assert_eq!(
        accepted["result"],
        json!({
            "protocol": 1,
            "daemon": env!("CARGO_PKG_VERSION"),
            "omp": omfp_core::OMP_VERSION,
            "warnings": ["running anyway (--allow-omp-mismatch)"]
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

    let second = start_with(config(&data_dir, &[]), ports(&FakeLauncher::new())).await;
    let error = second.unwrap_err().to_string();
    assert!(error.contains("already running"), "{error}");
    assert!(error.contains(&first.handle.addr().to_string()), "{error}");

    first.handle.shutdown().await.unwrap();
    assert_eq!(discover(&data_dir).unwrap(), None);
    // The directory is free again.
    let again = start_with(config(&data_dir, &[]), ports(&FakeLauncher::new()))
        .await
        .unwrap();
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
    let mut web = daemon.client("omfp-ui/web").await;
    let mut desktop = daemon.client("omfp-ui/desktop").await;
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
    client.ok("repos.list", json!({})).await;

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
    let folder = daemon.dir.path().join("adopted");
    std::fs::create_dir_all(&folder).unwrap();

    let workspace = script
        .ok("workspaces.adopt", json!({ "path": folder }))
        .await;
    let thread = script
        .ok(
            "threads.create",
            json!({ "title": "from a script", "workspaceIds": [workspace["id"]] }),
        )
        .await;

    let expected = json!({ "createdBy": { "kind": "user" }, "client": "script/nightly" });
    assert_eq!(workspace["provenance"], expected);
    assert_eq!(thread["provenance"], expected);
    let mut ui = daemon.client("omfp-ui/web").await;
    let index = ui.ok("subscribe", json!({ "topic": "index" })).await;
    assert_eq!(index["state"]["threads"][0]["provenance"], expected);
    assert_eq!(index["state"]["workspaces"][0]["provenance"], expected);
}

/// Params with every field any method needs; methods ignore the others (PRT-012).
fn superset_params(thread: &str, workspace: &str) -> Value {
    json!({
        "threadId": thread, "workspaceId": workspace, "repositoryId": "01NOREPO", "repo": "01NOREPO",
        "path": "/nonexistent", "pr": "owner/repo#1", "title": "t", "text": "hello", "message": "hello",
        "provider": "fake", "modelId": "scripted", "level": "low", "entryId": "e1", "subagentId": "s1",
        "op": "get", "question": "why?", "command": "true", "interactionId": "i1",
        "response": { "cancelled": true }, "kind": "shell", "cols": 80, "rows": 24, "terminalId": "x",
        "topic": "index"
    })
}

// spec: ARC-007, PRT-033
#[tokio::test(flavor = "multi_thread")]
async fn every_method_is_served_to_a_script_client() {
    let daemon = daemon(&[]).await;
    let mut script = daemon.client("script/smoke").await;
    let thread = create_thread(&mut script).await;
    let folder = daemon.dir.path().join("ws");
    std::fs::create_dir_all(&folder).unwrap();
    let workspace = script
        .ok("workspaces.adopt", json!({ "path": folder }))
        .await;
    let params = superset_params(&thread, workspace["id"].as_str().unwrap());

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

// spec: DMN-029
#[tokio::test(flavor = "multi_thread")]
async fn attaching_a_workspace_live_sends_add_dir() {
    let daemon = daemon(&[]).await;
    let mut client = daemon.client("test").await;
    let thread = create_thread(&mut client).await;
    client
        .ok("thread.state", json!({ "threadId": thread }))
        .await;
    let folder = daemon.dir.path().join("live");
    std::fs::create_dir_all(&folder).unwrap();
    let workspace = client
        .ok("workspaces.adopt", json!({ "path": folder }))
        .await;

    let summary = client
        .ok(
            "threads.attach",
            json!({ "threadId": thread, "workspaceId": workspace["id"] }),
        )
        .await;

    assert_eq!(summary["workspaceIds"], json!([workspace["id"]]));
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
    assert_eq!(
        prompts,
        [format!("/add-dir {}", workspace["path"].as_str().unwrap())]
    );
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
