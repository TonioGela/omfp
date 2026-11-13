use std::sync::atomic::{AtomicBool, Ordering};

use futures::StreamExt as _;

use super::*;

/// Sets its flag when dropped: shows that a tool's body was dropped.
struct Dropped(Arc<AtomicBool>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

// spec: DMN-155, DMN-151
#[tokio::test(start_paused = true)]
async fn a_running_tool_call_streams_keep_alives_and_closing_the_stream_cancels_it() {
    let dropped = Arc::new(AtomicBool::new(false));
    let guard = Dropped(Arc::clone(&dropped));
    let body: crate::agent_tools::ToolFuture = Box::pin(async move {
        let _guard = guard;
        std::future::pending::<()>().await;
        Ok(String::new())
    });
    let response = tool_stream(
        json!(1),
        ToolCall {
            body,
            era: Era::Modern,
        },
    );
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/event-stream"
    );
    assert_eq!(response.headers()["x-accel-buffering"], "no");
    let mut frames = response.into_body().into_data_stream();

    let started = tokio::time::Instant::now();
    let first = frames.next().await.unwrap().unwrap();
    assert!(
        first.starts_with(b":") && first.ends_with(b"\n\n"),
        "{first:?}"
    );
    assert_eq!(started.elapsed(), KEEP_ALIVE);
    assert!(!dropped.load(Ordering::SeqCst));

    drop(frames);
    assert!(dropped.load(Ordering::SeqCst), "the tool's body is dropped");
}

// spec: DMN-155, AGT-050
#[tokio::test]
async fn a_finished_tool_call_is_one_message_event() {
    let body: crate::agent_tools::ToolFuture = Box::pin(async { Err("not_found: no".to_owned()) });
    let response = tool_stream(
        json!("a"),
        ToolCall {
            body,
            era: Era::Legacy,
        },
    );
    let bytes = to_bytes(response.into_body(), MAX_BODY).await.unwrap();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let data = text
        .strip_prefix("event: message\ndata: ")
        .and_then(|rest| rest.strip_suffix("\n\n"))
        .unwrap_or_else(|| panic!("{text:?}"));
    assert_eq!(
        serde_json::from_str::<Value>(data).unwrap(),
        json!({
            "jsonrpc": "2.0",
            "id": "a",
            "result": { "content": [{ "type": "text", "text": "not_found: no" }], "isError": true },
        })
    );
}

// spec: DMN-155
#[test]
fn only_one_json_rpc_request_or_notification_is_accepted() {
    let request = JsonRpc::parse(json!({
        "jsonrpc": "2.0", "id": 7, "method": "tools/list", "params": { "cursor": null }
    }))
    .unwrap();
    assert_eq!(request.id, Some(json!(7)));
    assert_eq!(request.method, "tools/list");
    let notification =
        JsonRpc::parse(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).unwrap();
    assert_eq!(notification.id, None);
    assert_eq!(notification.params, Map::new());

    for (message, reason) in [
        (
            json!([{ "jsonrpc": "2.0", "id": 1, "method": "ping" }]),
            "no batches",
        ),
        (json!({ "id": 1, "method": "ping" }), "`jsonrpc`"),
        (
            json!({ "jsonrpc": "2.0", "id": 1, "result": {} }),
            "no `method`",
        ),
        (
            json!({ "jsonrpc": "2.0", "id": {}, "method": "ping" }),
            "`id`",
        ),
        (
            json!({ "jsonrpc": "2.0", "id": 1, "method": "ping", "params": [] }),
            "`params`",
        ),
    ] {
        let error = JsonRpc::parse(message.clone()).expect_err(&message.to_string());
        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert_eq!(error.code, code::INVALID_REQUEST);
        assert!(
            error.message.contains(reason),
            "{message}: {}",
            error.message
        );
    }
}

// spec: DMN-156
#[test]
fn mcp_name_headers_may_be_base64_encoded() {
    assert_eq!(decode_header("omfp_call").as_deref(), Some("omfp_call"));
    assert_eq!(
        decode_header("=?base64?SGVsbG8sIOS4lueVjA==?=").as_deref(),
        Some("Hello, 世界")
    );
    assert_eq!(decode_header("=?base64?!!?="), None);
}

// spec: DMN-157
#[test]
fn sessions_remember_their_client_and_forget_the_least_recently_used_beyond_256() {
    let sessions = Sessions::default();
    let first = sessions.open("first".to_owned()).unwrap();
    assert_eq!(first.len(), 32);
    let second = sessions.open("second".to_owned()).unwrap();
    assert_ne!(first, second);
    for index in 2..MAX_SESSIONS {
        sessions.open(format!("client {index}")).unwrap();
    }
    // Using the first makes the second the least recently used.
    assert_eq!(sessions.client(&first).as_deref(), Some("first"));

    sessions.open("one more".to_owned()).unwrap();

    assert_eq!(sessions.client(&second), None);
    assert_eq!(sessions.client(&first).as_deref(), Some("first"));
    assert!(sessions.end(&first));
    assert!(!sessions.end(&first));
    assert_eq!(sessions.client(&first), None);
}

// spec: AGT-050, AGT-052, DMN-198
#[test]
fn every_tool_and_uri_an_mcp_client_reads_about_exists_over_mcp() {
    use crate::agent_tools::{mentions, registry};

    let registry = registry();
    let tools: Vec<&str> = registry.iter().map(|tool| tool.name).collect();
    let uris = methods::resource_uris();
    for uri in mentions::STATIC_URIS {
        assert!(uris.contains(&uri), "{uri} is no MCP resource: {uris:?}");
    }
    let index = crate::widgets::shell::index();
    for uri in mentions::STATIC_URIS {
        assert!(
            uri == crate::widgets::shell::INDEX_URL || index.contains(&format!("- {uri} — ")),
            "the docs index has no line for {uri}: {index}"
        );
    }

    let mut texts = mentions::static_texts();
    texts.push(("instructions".to_owned(), methods::INSTRUCTIONS.to_owned()));
    for tool in &registry {
        texts.push((
            tool.name.to_owned(),
            format!("{} {}", tool.description, tool.parameters),
        ));
    }
    let missing: Vec<String> = texts
        .iter()
        .flat_map(|(served, text)| mentions::missing(served, text, &tools, &uris))
        .collect();
    assert_eq!(missing, Vec::<String>::new());
}
