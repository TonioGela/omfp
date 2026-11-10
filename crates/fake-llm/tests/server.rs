//! Behavior of the fake LLM over HTTP, as omp sees it (spec: TST-020).

use fake_llm::{FakeLlm, Matcher, Reply, Script, Step, ToolCall};
use serde_json::{Value, json};

const MODEL: &str = "scripted";

async fn post_chat(server: &FakeLlm, body: &Value) -> reqwest::Response {
    reqwest::Client::new()
        .post(format!("{}/chat/completions", server.base_url()))
        .json(body)
        .send()
        .await
        .expect("request reaches the fake LLM")
}

fn chat_request(user_text: &str, stream: bool) -> Value {
    json!({
        "model": MODEL,
        "stream": stream,
        "messages": [
            { "role": "system", "content": "be terse" },
            { "role": "user", "content": user_text }
        ]
    })
}

/// Parses an SSE body into its `data:` payloads, asserting the framing on the way.
fn sse_payloads(body: &str) -> Vec<String> {
    assert!(
        body.ends_with("\n\n"),
        "every SSE event ends with a blank line: {body:?}"
    );
    body.split("\n\n")
        .filter(|event| !event.is_empty())
        .map(|event| {
            event
                .strip_prefix("data: ")
                .unwrap_or_else(|| panic!("event without `data: ` prefix: {event:?}"))
                .to_owned()
        })
        .collect()
}

fn json_chunks(payloads: &[String]) -> Vec<Value> {
    payloads
        .iter()
        .filter(|payload| payload.as_str() != "[DONE]")
        .map(|payload| serde_json::from_str(payload).expect("chunk is JSON"))
        .collect()
}

// spec: TST-020
#[tokio::test]
async fn streams_scripted_text_as_chat_completion_chunks_ending_with_done() {
    let server = FakeLlm::start(Script::new(
        MODEL,
        vec![Step::reply(Reply::text("Hello from the fake model"))],
    ))
    .await
    .unwrap();

    let response = post_chat(&server, &chat_request("say hi", true)).await;

    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    let payloads = sse_payloads(&response.text().await.unwrap());
    assert_eq!(payloads.last().map(String::as_str), Some("[DONE]"));

    let chunks = json_chunks(&payloads);
    let content: String = chunks
        .iter()
        .filter_map(|chunk| chunk["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(content, "Hello from the fake model");
    assert!(
        chunks.len() > 3,
        "text is split over several chunks, got {}",
        chunks.len()
    );
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk["object"] == "chat.completion.chunk" && chunk["model"] == MODEL)
    );

    let finish: Vec<&str> = chunks
        .iter()
        .filter_map(|chunk| chunk["choices"][0]["finish_reason"].as_str())
        .collect();
    assert_eq!(finish, ["stop"]);
    let usage = chunks
        .iter()
        .find(|chunk| chunk.get("usage").is_some_and(|u| !u.is_null()));
    assert!(usage.is_some(), "a usage chunk is sent before [DONE]");
}

// spec: TST-020
#[tokio::test]
async fn streams_tool_calls_with_arguments_split_across_chunks() {
    let call = ToolCall::new(
        "call_1",
        "bash",
        json!({ "command": "echo hello world from a long command" }),
    );
    let server = FakeLlm::start(Script::new(
        MODEL,
        vec![Step::reply(Reply::tool_calls(vec![call]))],
    ))
    .await
    .unwrap();

    let payloads = sse_payloads(
        &post_chat(&server, &chat_request("run it", true))
            .await
            .text()
            .await
            .unwrap(),
    );
    let chunks = json_chunks(&payloads);

    let deltas: Vec<&Value> = chunks
        .iter()
        .filter_map(|chunk| chunk["choices"][0]["delta"]["tool_calls"].as_array())
        .flatten()
        .collect();
    assert!(deltas.len() > 2, "arguments arrive in several fragments");
    assert_eq!(deltas[0]["id"], "call_1");
    assert_eq!(deltas[0]["type"], "function");
    assert_eq!(deltas[0]["function"]["name"], "bash");
    assert!(deltas.iter().all(|delta| delta["index"] == 0));
    let arguments: String = deltas
        .iter()
        .filter_map(|delta| delta["function"]["arguments"].as_str())
        .collect();
    assert_eq!(
        serde_json::from_str::<Value>(&arguments).unwrap(),
        json!({ "command": "echo hello world from a long command" })
    );

    let finish: Vec<&str> = chunks
        .iter()
        .filter_map(|chunk| chunk["choices"][0]["finish_reason"].as_str())
        .collect();
    assert_eq!(finish, ["tool_calls"]);
}

// spec: TST-020
#[tokio::test]
async fn answers_non_streaming_requests_with_a_complete_message() {
    let server = FakeLlm::start(Script::new(
        MODEL,
        vec![Step::reply(Reply::text("short answer"))],
    ))
    .await
    .unwrap();

    let body: Value = post_chat(&server, &chat_request("hi", false))
        .await
        .json()
        .await
        .unwrap();

    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["choices"][0]["message"]["role"], "assistant");
    assert_eq!(body["choices"][0]["message"]["content"], "short answer");
    assert_eq!(body["choices"][0]["finish_reason"], "stop");
}

// spec: TST-020
#[tokio::test]
async fn records_every_request_body_in_order() {
    let server = FakeLlm::start(Script::new(
        MODEL,
        vec![
            Step::reply(Reply::text("one")),
            Step::reply(Reply::text("two")),
        ],
    ))
    .await
    .unwrap();

    post_chat(&server, &chat_request("first", true))
        .await
        .text()
        .await
        .unwrap();
    post_chat(&server, &chat_request("second", true))
        .await
        .text()
        .await
        .unwrap();

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["messages"][1]["content"], "first");
    assert_eq!(requests[1]["messages"][1]["content"], "second");
}

// spec: TST-020
#[tokio::test]
async fn replies_only_when_the_matcher_accepts_the_last_user_message() {
    let server = FakeLlm::start(Script::new(
        MODEL,
        vec![Step::when(
            Matcher::LastUserContains("weather".into()),
            Reply::text("sunny"),
        )],
    ))
    .await
    .unwrap();

    let rejected = post_chat(&server, &chat_request("tell me a joke", true)).await;
    assert_eq!(rejected.status(), 500);
    assert_eq!(
        server.failures().len(),
        1,
        "the mismatch is reported for the test to fail on"
    );

    let accepted = post_chat(&server, &chat_request("what's the weather?", true)).await;
    assert_eq!(accepted.status(), 200);
}

// spec: TST-020
#[tokio::test]
async fn reports_requests_beyond_the_end_of_the_script() {
    let server = FakeLlm::start(Script::new(
        MODEL,
        vec![Step::reply(Reply::text("only answer"))],
    ))
    .await
    .unwrap();

    post_chat(&server, &chat_request("first", true))
        .await
        .text()
        .await
        .unwrap();
    let extra = post_chat(&server, &chat_request("second", true)).await;

    assert_eq!(extra.status(), 500);
    let failures = server.failures();
    assert_eq!(failures.len(), 1);
    assert!(
        failures[0].contains("second"),
        "the failure shows the unexpected request: {}",
        failures[0]
    );
    assert!(!server.is_exhausted_cleanly());
}

// spec: TST-020
#[tokio::test]
async fn lists_the_scripted_model() {
    let server = FakeLlm::start(Script::new(MODEL, vec![])).await.unwrap();

    let body: Value = reqwest::get(format!("{}/models", server.base_url()))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(body["object"], "list");
    assert_eq!(body["data"][0]["id"], MODEL);
}

// spec: TST-020
#[tokio::test]
async fn is_exhausted_cleanly_only_after_every_step_was_consumed_without_failures() {
    let server = FakeLlm::start(Script::new(MODEL, vec![Step::reply(Reply::text("done"))]))
        .await
        .unwrap();
    assert!(!server.is_exhausted_cleanly());

    post_chat(&server, &chat_request("go", true))
        .await
        .text()
        .await
        .unwrap();

    assert!(server.is_exhausted_cleanly());
}

// spec: TST-020
#[test]
fn scripts_load_from_json() {
    let script: Script = serde_json::from_value(json!({
        "model": "scripted",
        "steps": [
            { "reply": { "text": "hi" } },
            { "when": { "last_user_contains": "run" },
              "reply": { "tool_calls": [ { "id": "call_9", "name": "read", "arguments": { "path": "a.txt" } } ] } }
        ]
    }))
    .unwrap();

    assert_eq!(script.model(), "scripted");
    assert_eq!(script.steps().len(), 2);
}
