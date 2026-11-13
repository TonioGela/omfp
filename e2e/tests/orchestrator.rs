//! The M10 acceptance journey (06-testing.md, E2E-20): a scripted orchestrating thread creates two workspaces, spawns
//! a thread with a prompt in each through `omfp_call`, waits for both with `omfp_thread_wait`, then mounts a dashboard
//! (inline code, `omfp_ui_mount`) showing both threads' live state with a button that prompts the orchestrator, and the
//! activity log widget beside it. The browser checks the dashboard, the live states, the click reaching the
//! orchestrator's next model request, the activity log, the "spawned by" filter and the cascade stop. Takes
//! `omfp-m10-*.png` (in the run's screenshot directory). Run with `just test-e2e`.

use e2e::Journey;
use fake_llm::{Matcher, Reply, Step, ToolCall};
use omfp_testkit::{FAKE_MODEL, FAKE_PROVIDER};
use serde_json::{Value, json};

/// The user's prompt to the orchestrator.
const ORCHESTRATE: &str = "ORCHESTRATE: run alpha and beta, then show me a dashboard";
/// The first messages of the two spawned threads, and the second round the orchestrator sends them.
const ALPHA_TASK: &str = "ALPHA-TASK: say you are done";
const BETA_TASK: &str = "BETA-TASK: say you are done";
const ALPHA_AGAIN: &str = "ALPHA-AGAIN: once more";
const BETA_AGAIN: &str = "BETA-AGAIN: once more";
/// What the dashboard's button sends the orchestrator.
const CLICKED: &str = "DASHBOARD-CLICK: run another round";

/// The dashboard the orchestrator mounts: each spawned thread's title and state from the window's index (live through
/// the SDK), and a button prompting the orchestrator with `thread.prompt`.
const DASHBOARD: &str = r#"// The threads an orchestrator runs and a button asking it for another round; props: orchestrator, threads, ask.
import { useState } from "react";
import { Button, Card } from "omfp/kit";
import { useCall, useThreadSummary } from "omfp/sdk";

type Summary = { title: string; settled: boolean; needsAttention: boolean; process: { state: string } };

function stateOf(thread: Summary): string {
  if (thread.needsAttention) return "awaiting";
  if (thread.process.state === "running") return thread.settled ? "idle" : "running";
  return thread.process.state;
}

function Row({ id }: { id: string }) {
  const thread = useThreadSummary(id) as Summary | undefined;
  const state = thread === undefined ? "unknown" : stateOf(thread);
  return (
    <li data-testid="dashboard-thread" data-thread-id={id} data-status={state}>
      {thread?.title ?? id}: {state}
    </li>
  );
}

export default function Dashboard({ orchestrator, threads = [], ask }: { orchestrator: string; threads?: string[]; ask: string }) {
  const call = useCall();
  const [asked, setAsked] = useState(0);
  return (
    <Card title="Orchestra">
      <ul data-testid="dashboard">
        {threads.map((id) => <Row key={id} id={id} />)}
      </ul>
      <Button
        data-testid="dashboard-ask"
        data-asked={asked}
        onPress={() => call("thread.prompt", { threadId: orchestrator, text: ask }).then(() => setAsked((n) => n + 1))}
      >
        Run another round
      </Button>
    </Card>
  );
}
"#;

/// Gated steps (0-based), released with ids only the daemon knows.
const SPAWN: usize = 1;
const WAIT: usize = 4;
const MOUNT: usize = 5;
const ROUND: usize = 8;
const ALPHA_ROUND: usize = 10;
const BETA_ROUND: usize = 11;

fn model() -> String {
    format!("{FAKE_PROVIDER}/{FAKE_MODEL}")
}

/// The orchestrator's turns and the spawned threads'; the threads run beside each other, so every step after the
/// first two answers in any order. The beta thread's second turn is never released: the cascade stop ends it.
fn script() -> Vec<Step> {
    let orchestrator = Matcher::LastUserContains(ORCHESTRATE.into());
    let clicked = Matcher::LastUserContains(CLICKED.into());
    let create = |id: &str, name: &str| {
        ToolCall::new(
            id,
            "omfp_call",
            json!({ "method": "workspaces.create", "params": { "name": name } }),
        )
    };
    let unused = || Reply::text("unused");
    vec![
        Step::when(
            orchestrator.clone(),
            Reply::tool_calls(vec![
                create("ws_alpha", "alpha"),
                create("ws_beta", "beta"),
            ]),
        ),
        Step::when(orchestrator.clone(), unused()).gated(),
        Step::when(
            Matcher::LastUserContains(ALPHA_TASK.into()),
            Reply::text("ALPHA-DONE"),
        )
        .any_order(),
        Step::when(
            Matcher::LastUserContains(BETA_TASK.into()),
            Reply::text("BETA-DONE"),
        )
        .any_order(),
        Step::when(orchestrator.clone(), unused())
            .gated()
            .any_order(),
        Step::when(orchestrator.clone(), unused())
            .gated()
            .any_order(),
        Step::when(
            orchestrator.clone(),
            Reply::tool_calls(vec![ToolCall::new(
                "mount_activity",
                "omfp_ui_mount",
                json!({ "id": "activity", "widget": "omfp:activity", "view": "dashboard", "place": "end" }),
            )]),
        )
        .any_order(),
        Step::when(orchestrator, Reply::text("DASHBOARD-READY")).any_order(),
        Step::when(clicked.clone(), unused()).gated().any_order(),
        Step::when(clicked, Reply::text("ROUND-STARTED")).any_order(),
        Step::when(
            Matcher::LastUserContains(ALPHA_AGAIN.into()),
            Reply::text("ALPHA-AGAIN-DONE"),
        )
        .gated()
        .any_order(),
        Step::when(
            Matcher::LastUserContains(BETA_AGAIN.into()),
            Reply::text("unused"),
        )
        .gated()
        .any_order(),
    ]
}

/// The text of the tool result answering `call_id`, from the first chat-completions request carrying it.
fn tool_result(requests: &[Value], call_id: &str) -> String {
    requests
        .iter()
        .filter_map(|request| request["messages"].as_array())
        .flatten()
        .find(|message| message["role"] == "tool" && message["tool_call_id"] == call_id)
        .map_or_else(
            || panic!("no result for {call_id} in {} requests", requests.len()),
            |message| message["content"].as_str().unwrap_or_default().to_owned(),
        )
}

/// The JSON result of the `omfp_call` answering `call_id`.
fn call_result(requests: &[Value], call_id: &str) -> Value {
    let text = tool_result(requests, call_id);
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{call_id}: {error}: {text}"))
}

fn id_of(value: &Value) -> String {
    value["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no id: {value:#}"))
        .to_owned()
}

/// The text of the last `user` message of a chat-completions request.
fn last_user(request: &Value) -> String {
    let message = request["messages"]
        .as_array()
        .and_then(|messages| {
            messages
                .iter()
                .rev()
                .find(|message| message["role"] == "user")
        })
        .unwrap_or_else(|| panic!("no user message: {request:#}"));
    match &message["content"] {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect(),
        other => panic!("user content: {other:#}"),
    }
}

/// Waits until the dashboard shows thread `id` as `status`.
async fn dashboard_shows(journey: &Journey, id: &str, status: &str) -> anyhow::Result<()> {
    let css = format!(
        "[data-testid=view][data-view=dashboard] [data-testid=dashboard-thread][data-thread-id=\"{id}\"]"
    );
    journey
        .until(
            &format!("the dashboard to show {id} {status}"),
            async |page| {
                Ok(
                    (page.attr(&css, "data-status").await?.as_deref() == Some(status))
                        .then_some(()),
                )
            },
        )
        .await
}

/// Waits until the activity widget lists `count` rows of `method`.
async fn activity_lists(journey: &Journey, method: &str, count: usize) -> anyhow::Result<()> {
    journey
        .until(
            &format!("{count} {method} rows in the activity log"),
            async |page| {
                let methods = page
                    .texts("[data-testid=activity] [data-testid=activity-method]")
                    .await?;
                Ok(
                    (methods.iter().filter(|shown| *shown == method).count() == count)
                        .then_some(()),
                )
            },
        )
        .await
}

/// The orchestrator's first turn, its gated steps released with the ids it learnt: two workspaces, a thread with its
/// first message in each (created by the orchestrator's agent), a wait for both, then the dashboard and the activity
/// log mounted in a new view. Returns the two threads' ids.
async fn orchestrate(journey: &Journey, orchestrator: &str) -> anyhow::Result<[String; 2]> {
    // Two workspaces, then a thread with its first message in each, created by the orchestrator's agent.
    journey.fake.wait_for_gate(SPAWN).await;
    let requests = journey.fake.requests();
    let workspaces = ["ws_alpha", "ws_beta"].map(|id| call_result(&requests, id));
    assert_eq!(
        workspaces.each_ref().map(|ws| ws["name"].clone()),
        [json!("alpha"), json!("beta")]
    );
    let spawn = |id: &str, workspace: &Value, title: &str, prompt: &str| {
        ToolCall::new(
            id,
            "omfp_call",
            json!({
                "method": "threads.create",
                "params": {
                    "workspace": { "id": id_of(workspace) },
                    "title": title,
                    "model": model(),
                    "prompt": prompt
                }
            }),
        )
    };
    journey.fake.release_gate_with(
        SPAWN,
        Reply::tool_calls(vec![
            spawn("spawn_alpha", &workspaces[0], "alpha worker", ALPHA_TASK),
            spawn("spawn_beta", &workspaces[1], "beta worker", BETA_TASK),
        ]),
    );

    // It waits for both.
    journey.fake.wait_for_gate(WAIT).await;
    let requests = journey.fake.requests();
    let spawned = ["spawn_alpha", "spawn_beta"].map(|id| call_result(&requests, id));
    for (thread, workspace) in spawned.iter().zip(&workspaces) {
        assert_eq!(thread["workspaceId"], workspace["id"], "{thread:#}");
        assert_eq!(
            thread["provenance"]["createdBy"],
            json!({ "kind": "thread", "threadId": orchestrator }),
            "{thread:#}"
        );
    }
    let [alpha, beta] = spawned.each_ref().map(id_of);
    let wait = |id: &str, thread: &str| {
        ToolCall::new(
            id,
            "omfp_thread_wait",
            json!({ "threadId": thread, "timeoutSeconds": 120 }),
        )
    };
    journey.fake.release_gate_with(
        WAIT,
        Reply::tool_calls(vec![wait("wait_alpha", &alpha), wait("wait_beta", &beta)]),
    );

    // Then it mounts the dashboard (with the ids it learnt) and the activity log in a new view.
    journey.fake.wait_for_gate(MOUNT).await;
    let requests = journey.fake.requests();
    for (call, done) in [("wait_alpha", "ALPHA-DONE"), ("wait_beta", "BETA-DONE")] {
        let waited = call_result(&requests, call);
        assert_eq!(waited["status"], "done", "{waited:#}");
        assert_eq!(waited["lastAssistantText"], done, "{waited:#}");
    }
    journey.fake.release_gate_with(
        MOUNT,
        Reply::tool_calls(vec![ToolCall::new(
            "mount_dashboard",
            "omfp_ui_mount",
            json!({
                "id": "dashboard",
                "code": DASHBOARD,
                "view": "dashboard",
                "props": { "orchestrator": orchestrator, "threads": [alpha, beta], "ask": CLICKED }
            }),
        )]),
    );
    journey.reply("DASHBOARD-READY").await?;
    let requests = journey.fake.requests();
    let mounted = tool_result(&requests, "mount_dashboard");
    assert!(
        mounted.starts_with(
            "Mounted ./widgets/dashboard as \"dashboard\" in view \"dashboard\"; open windows show it now."
        ),
        "{mounted}"
    );
    let mounted = tool_result(&requests, "mount_activity");
    assert!(
        mounted.starts_with("Mounted omfp:activity as \"activity\" in view \"dashboard\""),
        "{mounted}"
    );
    Ok([alpha, beta])
}

/// The dashboard shows both threads, and the activity log the agent's actions; its button prompts the orchestrator,
/// which runs another round of both threads, followed live by the dashboard.
async fn dashboard_round(
    journey: &Journey,
    orchestrator: &str,
    alpha: &str,
    beta: &str,
) -> anyhow::Result<()> {
    // The dashboard view shows both threads done, and the activity log the agent's actions (not its reads).
    journey.command("view dashboard").await?;
    journey
        .element("[data-testid=view][data-view=dashboard] [data-widget=\"./widgets/dashboard\"] [data-testid=dashboard]")
        .await?;
    dashboard_shows(journey, alpha, "idle").await?;
    dashboard_shows(journey, beta, "idle").await?;
    let titles = journey
        .texts("[data-testid=view] [data-testid=dashboard-thread]")
        .await?;
    assert_eq!(titles, ["alpha worker: idle", "beta worker: idle"]);
    journey
        .element("[data-testid=view][data-view=dashboard] [data-widget=\"omfp:activity\"] [data-testid=activity]")
        .await?;
    for method in ["workspaces.create", "threads.create", "views.mount"] {
        activity_lists(journey, method, 2).await?;
    }
    journey.screenshot("m10-orchestrator-dashboard").await?;

    // The button prompts the orchestrator: its next model request carries the click; it starts another round.
    journey.click("[data-testid=dashboard-ask]").await?;
    journey.fake.wait_for_gate(ROUND).await;
    let requests = journey.fake.requests();
    assert_eq!(last_user(requests.last().unwrap()), CLICKED);
    journey
        .until("the dashboard's prompt to be admitted", async |page| {
            let asked = page
                .attr("[data-testid=dashboard-ask]", "data-asked")
                .await?;
            Ok((asked.as_deref() == Some("1")).then_some(()))
        })
        .await?;
    let again = |id: &str, thread: &str, text: &str| {
        ToolCall::new(
            id,
            "omfp_call",
            json!({ "method": "thread.prompt", "params": { "threadId": thread, "text": text } }),
        )
    };
    journey.fake.release_gate_with(
        ROUND,
        Reply::tool_calls(vec![
            again("again_alpha", alpha, ALPHA_AGAIN),
            again("again_beta", beta, BETA_AGAIN),
        ]),
    );

    // Both threads run (their replies held), live on the dashboard; alpha finishes, beta keeps running.
    journey.fake.wait_for_gate(ALPHA_ROUND).await;
    journey.fake.wait_for_gate(BETA_ROUND).await;
    dashboard_shows(journey, alpha, "running").await?;
    dashboard_shows(journey, beta, "running").await?;
    journey.fake.release_gate(ALPHA_ROUND);
    dashboard_shows(journey, alpha, "idle").await?;
    dashboard_shows(journey, beta, "running").await?;
    journey.status(orchestrator, "idle").await?;
    // The user's own prompt from the dashboard is not an agent action; the orchestrator's two are.
    activity_lists(journey, "thread.prompt", 2).await?;
    journey.screenshot("m10-orchestrator-live").await?;
    Ok(())
}

// spec: E2E-20, TST-080, TST-116, AGT-041, AGT-043, AGT-061, AGT-070, AGT-071, AGT-072, UI-240, UI-241, UI-242
#[tokio::test(flavor = "multi_thread")]
async fn an_orchestrator_spawns_two_threads_and_its_dashboard_follows_and_prompts_it()
-> anyhow::Result<()> {
    let journey = Journey::start(script()).await?;
    journey.connected().await?;
    let bystander = journey.create_thread("bystander").await?;
    let orchestrator = journey.create_thread("orchestra").await?;
    journey.send(ORCHESTRATE).await?;
    let [alpha, beta] = orchestrate(&journey, &orchestrator).await?;
    dashboard_round(&journey, &orchestrator, &alpha, &beta).await?;

    // "Spawned by" on the orchestrator's row keeps its spawned tree in the left column.
    let filter = format!("[data-testid=spawned-filter][data-thread-id=\"{orchestrator}\"]");
    assert_eq!(journey.text(&filter).await?, "2");
    // Like a user's mouse, hover the row first: it then shows its delete × (UI-205), which moves the filter.
    journey
        .hover(&format!(
            "[data-testid=thread-row][data-thread-id=\"{orchestrator}\"]"
        ))
        .await?;
    journey.click(&filter).await?;
    journey.element("[data-testid=spawned-by]").await?;
    journey
        .until("the spawned tree alone in the left column", async |page| {
            let mut shown = page.thread_ids().await?;
            shown.sort();
            let mut tree = vec![orchestrator.clone(), alpha.clone(), beta.clone()];
            tree.sort();
            Ok((shown == tree).then_some(()))
        })
        .await?;
    journey.screenshot("m10-orchestrator-spawned-by").await?;

    // A cascade stop on the orchestrator stops it and both threads it spawned, the running one included.
    journey.command("stop!").await?;
    for thread in [&orchestrator, &alpha, &beta] {
        journey.status(thread, "stopped").await?;
    }
    dashboard_shows(&journey, &alpha, "stopped").await?;
    dashboard_shows(&journey, &beta, "stopped").await?;
    journey
        .until("the stop notice", async |page| {
            Ok(page
                .text("body")
                .await?
                .contains("Stopped 3 threads")
                .then_some(()))
        })
        .await?;
    journey.screenshot("m10-orchestrator-stopped").await?;
    journey.click("[data-testid=clear-spawned-by]").await?;
    journey.status(&bystander, "idle").await?;
    journey.finish("E2E-20").await
}
