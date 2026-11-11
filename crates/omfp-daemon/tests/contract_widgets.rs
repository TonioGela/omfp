//! M4 contract tests (L3): the real daemon with the pinned omp and the fake LLM — the `omfp://` host URIs every thread
//! reads (DMN-062, AGT-025) and the bash output omp streams while a command runs (AGT-020).

mod client;
mod common;

use clap::Parser;
use client::{Client, Follower};
use common::Harness;
use fake_llm::{Reply, Step, ToolCall};
use omfp_daemon::config::{Cli, Config};
use omfp_daemon::widgets::{DOCS, SCHEMA_URL};
use omfp_daemon::{DaemonHandle, Ports, start_with};
use omfp_testkit::{FAKE_MODEL, FAKE_PROVIDER, omp_bin};
use serde_json::{Value, json};
use tempfile::TempDir;

struct Daemon {
    handle: DaemonHandle,
    _data: TempDir,
}

impl Daemon {
    /// A daemon on the real ports with omp isolated by `harness`, without title generation (DMN-073).
    async fn start(harness: &Harness) -> Self {
        let data = tempfile::tempdir().unwrap();
        let omp = omp_bin();
        let workspaces = data.path().join("workspaces");
        let cli = Cli::try_parse_from([
            "daemon",
            "--bind",
            "127.0.0.1:0",
            "--data-dir",
            data.path().to_str().unwrap(),
            "--workspaces-root",
            workspaces.to_str().unwrap(),
            "--omp-bin",
            &omp,
            "--no-title-generation",
        ])
        .unwrap();
        let config = Config::from_cli(cli, None, None).unwrap();
        let mut ports = Ports::resolve(&config).unwrap();
        ports.omp_env = harness.env.env_overrides();
        let handle = start_with(config, ports).await.unwrap();
        Self {
            handle,
            _data: data,
        }
    }

    async fn client(&self) -> Client {
        Client::hello(self.handle.addr(), self.handle.token().expose(), "test").await
    }
}

/// A new thread on the fake model, followed from its snapshot.
async fn thread(client: &mut Client) -> (String, Follower) {
    let thread = client
        .ok(
            "threads.create",
            json!({ "title": "widgets", "model": format!("{FAKE_PROVIDER}/{FAKE_MODEL}") }),
        )
        .await;
    let id = thread["id"].as_str().unwrap().to_owned();
    let view = Follower::snapshot(client, &id, None).await;
    (id, view)
}

async fn prompt(client: &mut Client, thread: &str, text: &str) -> String {
    let result = client
        .ok("thread.prompt", json!({ "threadId": thread, "text": text }))
        .await;
    result["promptId"].as_str().unwrap().to_owned()
}

/// Applies the thread's events until one is the omp frame `kind` for which `matches` holds; returns that frame.
async fn until_frame(
    client: &mut Client,
    view: &mut Follower,
    kind: &str,
    matches: impl Fn(&Value) -> bool,
) -> Value {
    loop {
        let event = client.event(&view.topic).await;
        view.apply(&event);
        let frame = &event["event"]["frame"];
        if event["event"]["kind"] == "omp" && frame["type"] == kind && matches(frame) {
            return frame.clone();
        }
    }
}

/// The text of a tool result or partial result (`{ content: [{ type: "text", text }] }`).
fn text(result: &Value) -> String {
    result["content"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect()
        })
        .unwrap_or_default()
}

/// The text content of the tool result answering `call_id` in a chat-completions request.
fn tool_result(request: &Value, call_id: &str) -> String {
    request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|message| message["role"] == "tool" && message["tool_call_id"] == call_id)
        .map_or_else(
            || panic!("no result for {call_id}: {request:#}"),
            |message| message["content"].as_str().unwrap_or_default().to_owned(),
        )
}

// spec: DMN-062, AGT-025, PRT-030
#[tokio::test(flavor = "multi_thread")]
async fn the_agent_reads_the_widget_docs_and_the_views_schema_from_omfp_urls() {
    let harness = Harness::new(vec![
        Step::reply(Reply::tool_calls(vec![
            ToolCall::new("c1", "read", json!({ "path": "omfp://docs/widgets" })),
            ToolCall::new("c2", "read", json!({ "path": "omfp://schemas/views.json" })),
            ToolCall::new("c3", "read", json!({ "path": "omfp://nothing/here" })),
        ])),
        Step::reply(Reply::text("Read them.")),
    ])
    .await;
    let daemon = Daemon::start(&harness).await;
    let mut client = daemon.client().await;
    let (thread, mut view) = thread(&mut client).await;

    let prompt_id = prompt(&mut client, &thread, "learn widgets").await;
    view.until(&mut client, |state| {
        state.settled && state.prompts.contains_key(&prompt_id)
    })
    .await;

    let answer = &harness.fake.requests()[1];
    let docs = tool_result(answer, "c1");
    let first_lines: Vec<&str> = DOCS.lines().take(3).collect();
    for line in first_lines.iter().filter(|line| !line.is_empty()) {
        assert!(
            docs.contains(line),
            "{line:?} missing from the docs read: {docs}"
        );
    }
    let schema = tool_result(answer, "c2");
    assert!(
        schema.contains(&format!("\"$id\": \"{SCHEMA_URL}\"")),
        "{schema}"
    );
    let unknown = tool_result(answer, "c3");
    assert!(
        unknown.contains("omfp://docs/widgets") && unknown.contains(SCHEMA_URL),
        "an unknown URL names both: {unknown}"
    );
    harness.assert_no_stray_network();
}

// spec: AGT-020
#[tokio::test(flavor = "multi_thread")]
async fn omp_streams_the_bash_output_while_the_command_runs() {
    let gate_dir = tempfile::tempdir().unwrap();
    let gate = gate_dir.path().join("gate");
    let command = format!(
        "printf 'one\\n'; while [ ! -e {} ]; do sleep 0.05; done; printf 'two\\n'",
        gate.display()
    );
    let harness = Harness::new(vec![
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "c1",
            "bash",
            json!({ "command": command }),
        )])),
        Step::reply(Reply::text("Both lines.")),
    ])
    .await;
    let daemon = Daemon::start(&harness).await;
    let mut client = daemon.client().await;
    let (thread, mut view) = thread(&mut client).await;

    let prompt_id = prompt(&mut client, &thread, "run it").await;
    let update = until_frame(&mut client, &mut view, "tool_execution_update", |frame| {
        text(&frame["partialResult"]).contains("one")
    })
    .await;
    assert_eq!(update["toolCallId"], "c1");
    assert!(!text(&update["partialResult"]).contains("two"));

    std::fs::write(&gate, "").unwrap();
    let end = until_frame(&mut client, &mut view, "tool_execution_end", |frame| {
        frame["toolCallId"] == "c1"
    })
    .await;
    assert_eq!(text(&end["result"]), "one\ntwo\n");
    view.until(&mut client, |state| {
        state.settled && state.prompts.contains_key(&prompt_id)
    })
    .await;
    harness.assert_no_stray_network();
}
