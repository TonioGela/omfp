//! Contract scenarios (L3): realistic thread runs against the pinned omp, recorded as reducer fixtures under
//! `fixtures/threads/<omp version>/` with `just record-fixtures` (TST-012). The assertions pin down the omp behavior
//! the daemon and the reducer rely on.

mod common;

use common::{Harness, Recorder, TIMEOUT, frame_type, prompt, start};
use fake_llm::{Reply, Step, ToolCall};
use omp_rpc::{
    AbortAndRestoreQueueCommand, ExtensionUiRequest, ExtensionUiResponse, FollowUpCommand,
    PromptStatus, RpcInbound, RpcNotification, SelectUiRequest, ValueUiResponse,
};
use pmo_core::{Resolution, ThreadEvent};
use serde_json::json;

fn settled(count: usize) -> impl Fn(&[ThreadEvent]) -> bool {
    move |events| Recorder::count(events, "session_settled") >= count
}

/// Answer for the form fallback's `select` dialogs: SQLite for the radio group; Auth, then Search, then Done for
/// the checkbox loop (`picked_before` = answers already given to this field).
fn choose(select: &SelectUiRequest, picked_before: usize) -> String {
    let option = |needle: &str| {
        select
            .options
            .iter()
            .find(|option| option.contains(needle))
            .cloned()
            .unwrap_or_else(|| panic!("no `{needle}` option in {:?}", select.options))
    };
    if select.title.contains("database") {
        option("SQLite")
    } else {
        option(["Auth", "Search", "Done"][picked_before.min(2)])
    }
}

/// The first pending dialog request of `events` matching `pick`.
fn find_request<T>(
    events: &[ThreadEvent],
    pick: impl Fn(&ExtensionUiRequest) -> Option<T>,
) -> Option<T> {
    events.iter().find_map(|event| match event {
        ThreadEvent::Omp {
            frame: RpcNotification::ExtensionUiRequest(request),
        } => pick(request),
        _ => None,
    })
}

fn resolved(id: &str) -> ThreadEvent {
    ThreadEvent::InteractionResolved {
        id: id.into(),
        resolution: Resolution::Client {
            client: "contract-test".into(),
        },
    }
}

// spec: TST-012
#[tokio::test(flavor = "multi_thread")]
async fn text_reply() {
    let harness = Harness::new(vec![Step::reply(Reply::text("Hello from the fake model"))]).await;
    let spec = harness.spec(&[]);

    let recorder = tokio::task::spawn_blocking(move || {
        let (mut process, startup) = start(&spec);
        let recorder = Recorder::attach(&mut process, &startup);
        let turn = process
            .client()
            .prompt_and_wait(&prompt("say hi"), TIMEOUT)
            .unwrap();
        assert_eq!(turn.result.unwrap().status, PromptStatus::Completed);
        recorder.wait_until("session_settled", settled(1));
        recorder
    })
    .await
    .unwrap();

    let types: Vec<String> = recorder.events().iter().filter_map(frame_type).collect();
    assert!(
        types.iter().any(|kind| kind == "message_update"),
        "{types:?}"
    );
    recorder.record(&harness, "text_reply");
    harness.assert_no_stray_network();
}

// spec: TST-012
#[tokio::test(flavor = "multi_thread")]
async fn bash_tool_call() {
    let harness = Harness::new(vec![
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_bash_1",
            "bash",
            json!({ "command": "echo tool-output-123" }),
        )])),
        Step::reply(Reply::text("The command printed tool-output-123.")),
    ])
    .await;
    let mut spec = harness.spec(&[]);
    spec.approval_mode = Some("yolo".into());

    let recorder = tokio::task::spawn_blocking(move || {
        let (mut process, startup) = start(&spec);
        let recorder = Recorder::attach(&mut process, &startup);
        process
            .client()
            .prompt_and_wait(&prompt("run echo"), TIMEOUT)
            .unwrap();
        recorder.wait_until("session_settled", settled(1));
        recorder
    })
    .await
    .unwrap_or_else(|_| panic!("stderr:\n{}", harness.stderr()));

    let types: Vec<String> = recorder.events().iter().filter_map(frame_type).collect();
    assert!(
        types.iter().any(|kind| kind == "tool_execution_end"),
        "{types:?}"
    );
    let second = harness.fake.requests()[1].to_string();
    assert!(
        second.contains("tool-output-123"),
        "the tool result reaches the model"
    );
    recorder.record(&harness, "bash_tool_call");
    harness.assert_no_stray_network();
}

// spec: TST-012, DMN-050
#[tokio::test(flavor = "multi_thread")]
async fn approval_requested_and_granted() {
    let harness = Harness::new(vec![
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_bash_2",
            "bash",
            json!({ "command": "echo approved-run" }),
        )])),
        Step::reply(Reply::text("Done after approval.")),
    ])
    .await;
    let mut spec = harness.spec(&[]);
    spec.approval_mode = Some("always-ask".into());

    let recorder = tokio::task::spawn_blocking(move || {
        let (mut process, startup) = start(&spec);
        let recorder = Recorder::attach(&mut process, &startup);
        process.client().call(&prompt("run it")).unwrap();
        recorder.wait_until("an approval select", |events| {
            find_request(events, |request| match request {
                ExtensionUiRequest::Select(_) => Some(()),
                _ => None,
            })
            .is_some()
        });
        let (id, approve) = find_request(&recorder.events(), |request| match request {
            ExtensionUiRequest::Select(select) => select
                .options
                .iter()
                .find(|option| option.starts_with("Approve"))
                .map(|option| (select.id.clone(), option.clone())),
            _ => None,
        })
        .expect("an Approve option is offered");
        process
            .client()
            .send(&RpcInbound::ExtensionUiResponse(
                ExtensionUiResponse::ValueUiResponse(ValueUiResponse {
                    id: id.clone(),
                    value: approve,
                }),
            ))
            .unwrap();
        recorder.push(resolved(&id));
        recorder.wait_until("session_settled", settled(1));
        recorder
    })
    .await
    .unwrap();

    let second = harness.fake.requests()[1].to_string();
    assert!(second.contains("approved-run"), "the approved command ran");
    recorder.record(&harness, "approval_granted");
    harness.assert_no_stray_network();
}

// spec: TST-012, DMN-054
#[tokio::test(flavor = "multi_thread")]
async fn ask_answered() {
    // omp 18.18.6's `ask` takes one HTML form; RPC hosts receive it as `ask` questions (set_ask_dialog).
    let form = r#"<form>
  <fieldset><legend>Which database?</legend>
    <label><input type="radio" name="db" value="postgres"> Postgres</label>
    <label><input type="radio" name="db" value="sqlite" checked> SQLite</label>
  </fieldset>
  <fieldset><legend>Which features?</legend>
    <label><input type="checkbox" name="features" value="auth"> Auth</label>
    <label><input type="checkbox" name="features" value="billing"> Billing</label>
    <label><input type="checkbox" name="features" value="search"> Search</label>
  </fieldset>
</form>"#;
    let harness = Harness::new(vec![
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_ask_1",
            "ask",
            json!({ "html": form }),
        )])),
        Step::reply(Reply::text("Using SQLite with Auth and Search.")),
    ])
    .await;
    let spec = harness.spec(&[]);

    let (recorder, titles) = tokio::task::spawn_blocking(move || {
        let (mut process, startup) = start(&spec);
        let recorder = Recorder::attach(&mut process, &startup);
        process.client().call(&prompt("set it up")).unwrap();
        // omp 18.18.6 walks the form field by field with `select` dialogs (it does not use the RPC `ask` dialog):
        // the radio group, then the checkbox group as a loop that ends with "Done selecting".
        let mut answered: Vec<String> = Vec::new();
        let mut titles = Vec::new();
        loop {
            recorder.wait_until("a new select or the end of the turn", |events| {
                Recorder::count(events, "session_settled") >= 1
                    || find_request(events, |request| match request {
                        ExtensionUiRequest::Select(select) if !answered.contains(&select.id) => {
                            Some(())
                        }
                        _ => None,
                    })
                    .is_some()
            });
            let next = find_request(&recorder.events(), |request| match request {
                ExtensionUiRequest::Select(select) if !answered.contains(&select.id) => {
                    Some(select.clone())
                }
                _ => None,
            });
            let Some(select) = next else { break };
            let picked_before = titles
                .iter()
                .filter(|title| **title == select.title)
                .count();
            let choice = choose(&select, picked_before);
            titles.push(select.title.clone());
            process
                .client()
                .send(&RpcInbound::ExtensionUiResponse(
                    ExtensionUiResponse::ValueUiResponse(ValueUiResponse {
                        id: select.id.clone(),
                        value: choice,
                    }),
                ))
                .unwrap();
            recorder.push(resolved(&select.id));
            answered.push(select.id);
        }
        recorder.wait_until("session_settled", settled(1));
        (recorder, titles)
    })
    .await
    .unwrap();

    assert_eq!(titles.first().map(String::as_str), Some("Which database?"));
    assert!(
        titles.iter().any(|title| title.contains("features")),
        "{titles:?}"
    );
    let second = harness.fake.requests()[1].to_string().to_lowercase();
    assert!(
        second.contains("sqlite") && second.contains("search") && second.contains("auth"),
        "answers reach the model: {second}"
    );
    recorder.record(&harness, "ask_answered");
    harness.assert_no_stray_network();
}

// spec: TST-012
#[tokio::test(flavor = "multi_thread")]
async fn follow_up_queued_then_abort_restores_it() {
    let harness = Harness::new(vec![Step::reply(Reply::text("slow answer")).gated()]).await;
    let spec = harness.spec(&[]);
    let (process, recorder) = tokio::task::spawn_blocking(move || {
        let (mut process, startup) = start(&spec);
        let recorder = Recorder::attach(&mut process, &startup);
        process.client().call(&prompt("first")).unwrap();
        (process, recorder)
    })
    .await
    .unwrap();
    harness.fake.wait_for_gate(0).await;

    let (process, restored, recorder) = tokio::task::spawn_blocking(move || {
        process
            .client()
            .call(&FollowUpCommand {
                message: "later please".into(),
                images: None,
            })
            .unwrap();
        recorder.wait_until("a queue_update with the follow-up", |events| {
            Recorder::count(events, "queue_update") >= 1
        });
        let restored = process
            .client()
            .call(&AbortAndRestoreQueueCommand {})
            .unwrap();
        (process, restored, recorder)
    })
    .await
    .unwrap();
    harness.fake.release_gate(0);
    let recorder = tokio::task::spawn_blocking(move || {
        recorder.wait_until("session_settled", settled(1));
        drop(process);
        recorder
    })
    .await
    .unwrap();

    let texts: Vec<&str> = restored
        .follow_up
        .iter()
        .map(|message| message.text.as_str())
        .collect();
    assert_eq!(texts, ["later please"]);
    assert_eq!(restored.steering, Vec::new());
    recorder.record(&harness, "follow_up_then_abort");
}

// spec: TST-012, DMN-032
#[tokio::test(flavor = "multi_thread")]
async fn restart_resumes_with_history() {
    let harness = Harness::new(vec![
        Step::reply(Reply::text("first answer")),
        Step::reply(Reply::text("second answer")),
    ])
    .await;
    let spec = harness.spec(&[]);

    let recorder = tokio::task::spawn_blocking(move || {
        let (process, _) = start(&spec);
        process
            .client()
            .prompt_and_wait(&prompt("first question"), TIMEOUT)
            .unwrap();
        drop(process);
        let (mut process, startup) = start(&spec);
        let recorder = Recorder::attach(&mut process, &startup);
        process
            .client()
            .prompt_and_wait(&prompt("second question"), TIMEOUT)
            .unwrap();
        recorder.wait_until("session_settled", settled(1));
        recorder
    })
    .await
    .unwrap();

    let ThreadEvent::History { session, messages } = &recorder.events()[0] else {
        panic!("recording starts with the history");
    };
    assert!(session.resumed);
    assert_eq!(messages.len(), 2);
    recorder.record(&harness, "restart_with_history");
    harness.assert_no_stray_network();
}
