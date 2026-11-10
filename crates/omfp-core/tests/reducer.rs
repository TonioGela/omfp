//! Example tests for every event kind the reducer handles (TST-010).

mod common;

use common::{assistant_text, event, message_text, omp, streaming_text};
use omfp_core::wire::{
    AssistantContent, ExtensionUiRequest, NotifyType, PromptStatus, SubagentLifecycleStatus,
};
use omfp_core::{NoticeFrame, ProcessState, QueueState, ThreadState, ToolStatus, replay};
use serde_json::{Value, json};

fn user_message(text: &str) -> Value {
    json!({ "role": "user", "content": [{ "type": "text", "text": text }], "timestamp": 0 })
}

fn assistant_message(text: &str) -> Value {
    json!({ "role": "assistant", "content": [{ "type": "text", "text": text }], "stopReason": "stop", "timestamp": 0 })
}

fn message_start(id: &str, message: Value) -> omfp_core::ThreadEvent {
    let mut frame = json!({ "type": "message_start", "messageId": id });
    frame["message"] = message;
    omp(frame)
}

fn message_end(id: &str, message: Value) -> omfp_core::ThreadEvent {
    let mut frame = json!({ "type": "message_end", "messageId": id });
    frame["message"] = message;
    omp(frame)
}

fn update(id: &str, assistant_event: Value) -> omfp_core::ThreadEvent {
    let mut frame = json!({
        "type": "message_update",
        "messageId": id,
        "message": { "role": "assistant" },
    });
    frame["assistantMessageEvent"] = assistant_event;
    omp(frame)
}

fn process(state: Value) -> omfp_core::ThreadEvent {
    let mut value = json!({ "kind": "process" });
    value["state"] = state;
    event(value)
}

fn ui_request(fields: Value) -> omfp_core::ThreadEvent {
    let mut frame = json!({ "type": "extension_ui_request" });
    if let (Some(frame), Value::Object(fields)) = (frame.as_object_mut(), fields) {
        frame.extend(fields);
    }
    omp(frame)
}

fn select(id: &str) -> omfp_core::ThreadEvent {
    ui_request(json!({ "id": id, "method": "select", "title": "Pick", "options": ["a", "b"] }))
}

fn interaction_ids(state: &ThreadState) -> Vec<String> {
    state
        .interactions
        .iter()
        .map(|request| match request {
            ExtensionUiRequest::Select(r) => r.id.clone(),
            ExtensionUiRequest::Confirm(r) => r.id.clone(),
            ExtensionUiRequest::Input(r) => r.id.clone(),
            ExtensionUiRequest::Editor(r) => r.id.clone(),
            ExtensionUiRequest::Ask(r) => r.id.clone(),
            other => format!("unexpected {other:?}"),
        })
        .collect()
}

fn transcript_texts(state: &ThreadState) -> Vec<String> {
    state
        .transcript
        .iter()
        .map(|entry| message_text(&entry.message))
        .collect()
}

fn history(messages: Value) -> omfp_core::ThreadEvent {
    let mut value = json!({
        "kind": "history",
        "session": { "session_id": "s1", "resumed": true },
    });
    value["messages"] = messages;
    event(value)
}

fn first_block(state: &ThreadState) -> Option<&AssistantContent> {
    state.streaming.as_ref()?.message.content.as_ref()?.first()
}

// spec: ARC-006
#[test]
fn a_new_thread_is_stopped_settled_and_empty() {
    let state = ThreadState::default();
    assert_eq!(state.process, ProcessState::Stopped);
    assert!(state.settled);
    assert_eq!(state.transcript, Vec::new());
    assert_eq!(state.streaming, None);
    assert_eq!(replay([]), state);
}

// spec: TST-010, DMN-032
#[test]
fn history_replaces_transcript_and_binds_the_session() {
    let state = replay(&[
        message_end("m1", user_message("old")),
        message_start("m2", assistant_message("partial")),
        history(json!([user_message("q"), assistant_message("a")])),
    ]);
    assert_eq!(transcript_texts(&state), ["q", "a"]);
    assert_eq!(state.transcript[0].message_id, None);
    assert_eq!(state.streaming, None);
    let session = state.session.expect("session");
    assert_eq!(session.session_id, "s1");
    assert!(session.resumed);
}

// spec: TST-010
#[test]
fn process_state_follows_process_events() {
    let state = replay(&[
        process(json!({ "state": "starting" })),
        process(json!({ "state": "running" })),
    ]);
    assert_eq!(state.process, ProcessState::Running);
}

// spec: TST-010
#[test]
fn agent_start_unsettles_and_session_settled_settles() {
    let busy = replay(&[
        process(json!({ "state": "running" })),
        omp(json!({ "type": "agent_start" })),
    ]);
    assert!(!busy.settled);
    let settled = replay(&[
        process(json!({ "state": "running" })),
        omp(json!({ "type": "agent_start" })),
        omp(json!({ "type": "agent_end", "messages": [] })),
    ]);
    assert!(!settled.settled, "agent_end alone does not settle");
    let mut state = settled;
    state.apply(&omp(json!({ "type": "session_settled" })));
    assert!(state.settled);
}

// spec: TST-010, DMN-035
#[test]
fn leaving_running_settles_and_cancels_pending_interactions() {
    let running = [
        process(json!({ "state": "running" })),
        omp(json!({ "type": "agent_start" })),
        select("ui-1"),
        message_start("m1", assistant_message("half")),
    ];
    let failed = {
        let mut events = running.to_vec();
        events.push(process(
            json!({ "state": "failed", "message": "exited", "exit_code": 1 }),
        ));
        replay(&events)
    };
    assert!(failed.settled);
    assert_eq!(failed.interactions, Vec::new());
    assert_eq!(failed.streaming, None);
    assert_eq!(
        failed.process,
        ProcessState::Failed {
            message: "exited".into(),
            exit_code: Some(1),
            stderr_tail: String::new()
        }
    );

    for state in [
        json!({ "state": "stopped" }),
        json!({ "state": "terminal" }),
    ] {
        let mut events = running.to_vec();
        events.push(process(state));
        let left = replay(&events);
        assert!(left.settled);
        assert_eq!(left.interactions, Vec::new());
    }

    let mut restarting = running.to_vec();
    restarting.push(process(json!({ "state": "starting" })));
    let state = replay(&restarting);
    assert!(!state.settled);
    assert_eq!(interaction_ids(&state), ["ui-1"]);
}

// spec: TST-010, DMN-043
#[test]
fn message_end_appends_and_replaces_by_message_id() {
    let state = replay(&[
        message_end("m1", user_message("hi")),
        message_end("m2", assistant_message("first")),
        message_end("m2", assistant_message("revised")),
        omp(json!({ "type": "message_end", "message": user_message("no id") })),
        omp(json!({ "type": "message_end", "message": user_message("no id") })),
    ]);
    assert_eq!(
        transcript_texts(&state),
        ["hi", "revised", "no id", "no id"]
    );
    let ids: Vec<Option<&str>> = state
        .transcript
        .iter()
        .map(|entry| entry.message_id.as_deref())
        .collect();
    assert_eq!(ids, [Some("m1"), Some("m2"), None, None]);
}

// spec: TST-010, DMN-043
#[test]
fn agent_end_messages_are_not_the_transcript() {
    let state = replay(&[
        message_end("m1", user_message("kept")),
        omp(
            json!({ "type": "agent_end", "messages": [user_message("compacted"), assistant_message("away")] }),
        ),
    ]);
    assert_eq!(transcript_texts(&state), ["kept"]);
}

// spec: TST-010
#[test]
fn message_start_begins_streaming_only_for_assistant_messages() {
    let user = replay(&[message_start("m1", user_message("hi"))]);
    assert_eq!(user.streaming, None);

    let assistant = replay(&[message_start("m2", assistant_message("Hel"))]);
    let streaming = assistant.streaming.expect("streaming");
    assert_eq!(streaming.message_id.as_deref(), Some("m2"));
    assert_eq!(assistant_text(&streaming.message), "Hel");
}

// spec: TST-010
#[test]
fn text_deltas_rebuild_the_block_and_end_is_authoritative() {
    let mut state = replay(&[
        message_start(
            "m1",
            json!({ "role": "assistant", "content": [{ "type": "text", "text": "ahead" }] }),
        ),
        update(
            "m1",
            json!({ "type": "text_start", "contentIndex": 0, "block": { "type": "text", "text": "" } }),
        ),
        update(
            "m1",
            json!({ "type": "text_delta", "contentIndex": 0, "delta": "Hel" }),
        ),
        update(
            "m1",
            json!({ "type": "text_delta", "contentIndex": 0, "delta": "lo" }),
        ),
    ]);
    assert_eq!(streaming_text(&state).as_deref(), Some("Hello"));

    state.apply(&update(
        "m1",
        json!({
            "type": "text_end",
            "contentIndex": 0,
            "content": "Hello!",
            "block": { "type": "text", "textSignature": "sig" },
        }),
    ));
    let Some(AssistantContent::Text(text)) = first_block(&state) else {
        panic!("text block expected")
    };
    assert_eq!(text.text.as_deref(), Some("Hello!"));
    assert_eq!(text.text_signature.as_deref(), Some("sig"));
}

// spec: TST-010
#[test]
fn thinking_deltas_rebuild_the_block() {
    let mut state = replay(&[
        message_start("m1", json!({ "role": "assistant", "content": [] })),
        update(
            "m1",
            json!({ "type": "thinking_start", "contentIndex": 0, "block": { "type": "thinking", "thinking": "" } }),
        ),
        update(
            "m1",
            json!({ "type": "thinking_delta", "contentIndex": 0, "delta": "hmm " }),
        ),
        update(
            "m1",
            json!({ "type": "thinking_delta", "contentIndex": 0, "delta": "ok" }),
        ),
    ]);
    let Some(AssistantContent::Thinking(thinking)) = first_block(&state) else {
        panic!("thinking block expected")
    };
    assert_eq!(thinking.thinking.as_deref(), Some("hmm ok"));

    state.apply(&update(
        "m1",
        json!({
            "type": "thinking_end",
            "contentIndex": 0,
            "content": "hmm ok.",
            "block": { "type": "thinking", "thinkingSignature": "sig" },
        }),
    ));
    let Some(AssistantContent::Thinking(thinking)) = first_block(&state) else {
        panic!("thinking block expected")
    };
    assert_eq!(thinking.thinking.as_deref(), Some("hmm ok."));
    assert_eq!(thinking.thinking_signature.as_deref(), Some("sig"));
}

// spec: TST-010
#[test]
fn toolcall_deltas_accumulate_argument_json_until_end() {
    let mut state = replay(&[
        message_start("m1", json!({ "role": "assistant", "content": [] })),
        update(
            "m1",
            json!({
                "type": "toolcall_start",
                "contentIndex": 0,
                "block": { "type": "toolCall", "id": "c1", "name": "bash", "arguments": {}, "partialArgs": "{\"cm" },
            }),
        ),
        update(
            "m1",
            json!({ "type": "toolcall_delta", "contentIndex": 0, "delta": "{\"cmd\":" }),
        ),
        update(
            "m1",
            json!({ "type": "toolcall_delta", "contentIndex": 0, "delta": "\"ls\"}" }),
        ),
    ]);
    let Some(AssistantContent::ToolCall(call)) = first_block(&state) else {
        panic!("tool call expected")
    };
    assert_eq!(call.id.as_deref(), Some("c1"));
    assert_eq!(call.name.as_deref(), Some("bash"));
    assert_eq!(
        call.extra.get("partialArgs").and_then(Value::as_str),
        Some("{\"cmd\":\"ls\"}")
    );

    state.apply(&update(
        "m1",
        json!({
            "type": "toolcall_end",
            "contentIndex": 0,
            "toolCall": { "type": "toolCall", "id": "c1", "name": "bash", "arguments": { "cmd": "ls" } },
        }),
    ));
    let Some(AssistantContent::ToolCall(call)) = first_block(&state) else {
        panic!("tool call expected")
    };
    assert_eq!(
        call.arguments.as_ref().and_then(|args| args.get("cmd")),
        Some(&json!("ls"))
    );
}

// spec: TST-010
#[test]
fn image_end_places_the_whole_image() {
    let state = replay(&[
        message_start("m1", json!({ "role": "assistant", "content": [] })),
        update(
            "m1",
            json!({ "type": "image_end", "contentIndex": 0, "content": { "type": "image", "data": "AAAA", "mimeType": "image/png" } }),
        ),
    ]);
    let Some(AssistantContent::Image(image)) = first_block(&state) else {
        panic!("image expected")
    };
    assert_eq!(image.data.as_deref(), Some("AAAA"));
}

// spec: TST-010
#[test]
fn start_restarts_from_the_carried_message() {
    let state = replay(&[
        message_start("m1", assistant_message("first try")),
        update(
            "m1",
            json!({ "type": "start", "message": { "role": "assistant", "content": [] } }),
        ),
        update(
            "m1",
            json!({ "type": "text_start", "contentIndex": 0, "block": { "type": "text", "text": "" } }),
        ),
        update(
            "m1",
            json!({ "type": "text_delta", "contentIndex": 0, "delta": "again" }),
        ),
    ]);
    assert_eq!(streaming_text(&state).as_deref(), Some("again"));
}

// spec: TST-010
#[test]
fn deltas_without_message_start_begin_a_streaming_message() {
    let state = replay(&[
        update(
            "m9",
            json!({ "type": "text_start", "contentIndex": 0, "block": { "type": "text", "text": "" } }),
        ),
        update(
            "m9",
            json!({ "type": "text_delta", "contentIndex": 0, "delta": "x" }),
        ),
    ]);
    assert_eq!(
        state
            .streaming
            .as_ref()
            .and_then(|s| s.message_id.as_deref()),
        Some("m9")
    );
    assert_eq!(streaming_text(&state).as_deref(), Some("x"));
}

// spec: TST-010
#[test]
fn deltas_for_missing_or_mismatched_blocks_are_ignored() {
    let state = replay(&[
        message_start("m1", json!({ "role": "assistant", "content": [] })),
        update(
            "m1",
            json!({ "type": "text_delta", "contentIndex": 3, "delta": "lost" }),
        ),
        update(
            "m1",
            json!({ "type": "text_start", "contentIndex": 0, "block": { "type": "text", "text": "" } }),
        ),
        update(
            "m1",
            json!({ "type": "thinking_delta", "contentIndex": 0, "delta": "wrong kind" }),
        ),
        update(
            "m1",
            json!({ "type": "toolcall_delta", "contentIndex": 0, "delta": "wrong kind" }),
        ),
        update(
            "m1",
            json!({ "type": "text_delta", "contentIndex": -1, "delta": "negative" }),
        ),
        update("m1", json!({ "type": "text_delta", "delta": "no index" })),
    ]);
    assert_eq!(streaming_text(&state).as_deref(), Some(""));
    assert_eq!(
        state
            .streaming
            .and_then(|s| s.message.content)
            .map(|content| content.len()),
        Some(1)
    );
}

// spec: TST-010
#[test]
fn assistant_message_end_clears_streaming_but_injected_records_do_not() {
    let streaming = [
        message_start("m1", json!({ "role": "assistant", "content": [] })),
        update(
            "m1",
            json!({ "type": "text_start", "contentIndex": 0, "block": { "type": "text", "text": "" } }),
        ),
        message_end(
            "m2",
            json!({ "role": "custom", "customType": "irc", "content": "aside" }),
        ),
    ];
    let state = replay(&streaming);
    assert_eq!(
        state
            .streaming
            .as_ref()
            .and_then(|s| s.message_id.as_deref()),
        Some("m1")
    );
    assert_eq!(state.transcript.len(), 1);

    let mut ended = state;
    ended.apply(&message_end("m1", assistant_message("final")));
    assert_eq!(ended.streaming, None);
    assert_eq!(
        transcript_texts(&ended).last().map(String::as_str),
        Some("final")
    );
}

// spec: TST-010
#[test]
fn tool_runs_follow_execution_events() {
    let state = replay(&[
        omp(
            json!({ "type": "tool_execution_start", "toolCallId": "t1", "toolName": "bash", "args": { "cmd": "ls" }, "intent": "list" }),
        ),
        omp(
            json!({ "type": "tool_execution_start", "toolCallId": "t2", "toolName": "read", "args": {} }),
        ),
        omp(
            json!({ "type": "tool_execution_update", "toolCallId": "t1", "toolName": "bash", "partialResult": { "out": "a" } }),
        ),
        omp(
            json!({ "type": "tool_execution_update", "toolCallId": "t1", "toolName": "bash", "partialResult": { "out": "ab" } }),
        ),
        omp(
            json!({ "type": "tool_execution_end", "toolCallId": "t1", "toolName": "bash", "result": { "out": "abc" }, "isError": false }),
        ),
        omp(
            json!({ "type": "tool_execution_end", "toolCallId": "t2", "toolName": "read", "result": "nope", "isError": true }),
        ),
    ]);
    let ids: Vec<&str> = state
        .tools
        .iter()
        .map(|run| run.tool_call_id.as_str())
        .collect();
    assert_eq!(ids, ["t1", "t2"]);
    let bash = &state.tools[0];
    assert_eq!(bash.tool_name, "bash");
    assert_eq!(bash.args, Some(json!({ "cmd": "ls" })));
    assert_eq!(bash.intent.as_deref(), Some("list"));
    assert_eq!(bash.partial_result, Some(json!({ "out": "ab" })));
    assert_eq!(bash.result, Some(json!({ "out": "abc" })));
    assert_eq!(bash.status, ToolStatus::Succeeded);
    assert_eq!(state.tools[1].status, ToolStatus::Failed);
    assert_eq!(state.tools[1].result, Some(json!("nope")));
}

// spec: TST-010
#[test]
fn tool_stream_updates_preview_and_announce_runs() {
    let pending = replay(&[omp(
        json!({ "type": "tool_stream_update", "toolCallId": "t1", "toolName": "edit", "update": { "delta": "{" } }),
    )]);
    assert_eq!(pending.tools.len(), 1);
    assert_eq!(pending.tools[0].status, ToolStatus::Pending);
    assert_eq!(pending.tools[0].preview, Some(json!({ "delta": "{" })));

    let mut running = pending;
    running.apply(&omp(json!({ "type": "tool_execution_start", "toolCallId": "t1", "toolName": "edit", "args": { "p": 1 } })));
    running.apply(&omp(
        json!({ "type": "tool_stream_update", "toolCallId": "t1", "toolName": "edit", "update": { "delta": "}" } }),
    ));
    assert_eq!(running.tools.len(), 1);
    assert_eq!(running.tools[0].status, ToolStatus::Running);
    assert_eq!(running.tools[0].args, Some(json!({ "p": 1 })));
    assert_eq!(running.tools[0].preview, Some(json!({ "delta": "}" })));
}

// spec: TST-010
#[test]
fn tool_events_for_unknown_runs_create_them() {
    let state = replay(&[
        omp(
            json!({ "type": "tool_execution_update", "toolCallId": "u", "toolName": "bash", "partialResult": 1 }),
        ),
        omp(
            json!({ "type": "tool_execution_end", "toolCallId": "e", "toolName": "bash", "result": 2 }),
        ),
    ]);
    assert_eq!(state.tools[0].status, ToolStatus::Running);
    assert_eq!(state.tools[0].partial_result, Some(json!(1)));
    assert_eq!(state.tools[1].status, ToolStatus::Succeeded);
    assert_eq!(state.tools[1].result, Some(json!(2)));
}

// spec: TST-010, TST-011
#[test]
fn queue_equals_the_last_queue_update() {
    let state = replay(&[
        omp(
            json!({ "type": "queue_update", "steering": ["s1"], "followUp": ["f1", "f2"], "liveSteered": 1 }),
        ),
        omp(
            json!({ "type": "queue_update", "steering": [], "followUp": ["f2"], "liveSteered": 0 }),
        ),
    ]);
    assert_eq!(
        state.queue,
        QueueState {
            steering: Vec::new(),
            follow_up: vec!["f2".into()],
            live_steered: 0
        }
    );
}

// spec: TST-010, DMN-050
#[test]
fn dialog_requests_become_pending_interactions_in_order() {
    let state = replay(&[
        select("s"),
        ui_request(
            json!({ "id": "c", "method": "confirm", "title": "Sure?", "message": "Really" }),
        ),
        ui_request(json!({ "id": "i", "method": "input", "title": "Name" })),
        ui_request(json!({ "id": "e", "method": "editor", "title": "Edit", "promptStyle": true })),
        ui_request(json!({
            "id": "a",
            "method": "ask",
            "questions": [{ "id": "q", "question": "Which?", "options": [{ "label": "x" }] }],
        })),
    ]);
    assert_eq!(interaction_ids(&state), ["s", "c", "i", "e", "a"]);
}

// spec: TST-010, DMN-051
#[test]
fn interaction_resolved_closes_the_matching_request() {
    let state = replay(&[
        select("s1"),
        select("s2"),
        event(
            json!({ "kind": "interaction_resolved", "id": "s1", "resolution": { "by": "client", "client": "ui" } }),
        ),
        event(
            json!({ "kind": "interaction_resolved", "id": "zz", "resolution": { "by": "client", "client": "ui" } }),
        ),
    ]);
    assert_eq!(interaction_ids(&state), ["s2"]);
}

// spec: TST-010, DMN-052
#[test]
fn cancel_closes_the_target_request() {
    let state = replay(&[
        select("s1"),
        select("s2"),
        ui_request(json!({ "id": "x", "method": "cancel", "targetId": "s2" })),
    ]);
    assert_eq!(interaction_ids(&state), ["s1"]);
}

// spec: TST-010, DMN-053
#[test]
fn presentation_requests_update_presentation_state() {
    let state = replay(&vec![
        ui_request(
            json!({ "id": "1", "method": "setStatus", "statusKey": "git", "statusText": "clean" }),
        ),
        ui_request(
            json!({ "id": "2", "method": "setStatus", "statusKey": "tmp", "statusText": "x" }),
        ),
        ui_request(json!({ "id": "3", "method": "setStatus", "statusKey": "tmp" })),
        ui_request(
            json!({ "id": "4", "method": "setWidget", "widgetKey": "w", "widgetLines": ["l1", "l2"], "widgetPlacement": "belowEditor" }),
        ),
        ui_request(
            json!({ "id": "5", "method": "setWidget", "widgetKey": "gone", "widgetLines": ["x"] }),
        ),
        ui_request(json!({ "id": "6", "method": "setWidget", "widgetKey": "gone" })),
        ui_request(json!({ "id": "7", "method": "setTitle", "title": "My thread" })),
        ui_request(json!({ "id": "8", "method": "set_editor_text", "text": "draft" })),
        ui_request(
            json!({ "id": "9", "method": "notify", "message": "hello", "notifyType": "warning" }),
        ),
        ui_request(
            json!({ "id": "10", "method": "open_url", "url": "https://example.test/login" }),
        ),
    ]);
    let presentation = &state.presentation;
    assert_eq!(
        presentation.statuses.iter().collect::<Vec<_>>(),
        [(&"git".to_string(), &"clean".to_string())]
    );
    assert_eq!(presentation.widgets.keys().collect::<Vec<_>>(), ["w"]);
    assert_eq!(presentation.widgets["w"].lines, ["l1", "l2"]);
    assert_eq!(presentation.title.as_deref(), Some("My thread"));
    assert_eq!(presentation.editor_text.as_deref(), Some("draft"));
    assert_eq!(presentation.notifications.len(), 1);
    assert_eq!(presentation.notifications[0].message, "hello");
    assert_eq!(
        presentation.notifications[0].notify_type,
        Some(NotifyType::Warning)
    );
    assert_eq!(
        presentation.open_url.as_ref().map(|r| r.url.as_str()),
        Some("https://example.test/login")
    );
    assert_eq!(
        state.interactions,
        Vec::new(),
        "presentation requests are not interactions"
    );
}

// spec: TST-010
#[test]
fn available_commands_update_replaces_commands() {
    let state = replay(&[
        omp(
            json!({ "type": "available_commands_update", "commands": [{ "name": "old", "source": "builtin" }] }),
        ),
        omp(
            json!({ "type": "available_commands_update", "commands": [{ "name": "new", "source": "skill" }] }),
        ),
    ]);
    let names: Vec<&str> = state
        .commands
        .iter()
        .map(|command| command.name.as_str())
        .collect();
    assert_eq!(names, ["new"]);
}

// spec: TST-010, PRT-030
#[test]
fn prompt_results_are_kept_by_prompt_id() {
    let state = replay(&[
        omp(
            json!({ "type": "prompt_result", "id": "p1", "agentInvoked": true, "status": "completed", "sessionSettled": false }),
        ),
        omp(
            json!({ "type": "prompt_result", "id": "p2", "agentInvoked": true, "status": "error", "sessionSettled": true, "error": { "message": "boom", "retryable": false } }),
        ),
        omp(
            json!({ "type": "prompt_result", "agentInvoked": false, "status": "completed", "sessionSettled": true }),
        ),
    ]);
    assert_eq!(state.prompts.keys().collect::<Vec<_>>(), ["p1", "p2"]);
    assert_eq!(state.prompts["p2"].status, PromptStatus::Error);
}

// spec: TST-010
#[test]
fn subagent_frames_keep_the_latest_lifecycle_and_progress() {
    let state = replay(&[
        omp(
            json!({ "type": "subagent_lifecycle", "payload": { "id": "sa1", "agent": "task", "agentSource": "bundled", "status": "started", "index": 0 } }),
        ),
        omp(
            json!({ "type": "subagent_progress", "payload": { "index": 0, "agent": "task", "agentSource": "bundled", "task": "t", "progress": { "id": "sa1", "status": "running" } } }),
        ),
        omp(
            json!({ "type": "subagent_progress", "payload": { "index": 0, "agent": "task", "agentSource": "bundled", "task": "t", "progress": { "id": "sa1", "status": "completed" } } }),
        ),
        omp(
            json!({ "type": "subagent_lifecycle", "payload": { "id": "sa1", "agent": "task", "agentSource": "bundled", "status": "completed", "index": 0 } }),
        ),
        omp(
            json!({ "type": "subagent_progress", "payload": { "index": 1, "agent": "task", "agentSource": "bundled", "task": "t", "progress": {} } }),
        ),
    ]);
    assert_eq!(state.subagents.keys().collect::<Vec<_>>(), ["sa1"]);
    let subagent = &state.subagents["sa1"];
    assert_eq!(
        subagent.lifecycle.as_ref().map(|l| l.status),
        Some(SubagentLifecycleStatus::Completed)
    );
    assert_eq!(
        subagent
            .progress
            .as_ref()
            .and_then(|p| p.progress.get("status")),
        Some(&json!("completed"))
    );
}

// spec: TST-010, UI-043, DMN-042
#[test]
fn noteworthy_frames_become_positioned_notices() {
    let state = replay(&vec![
        omp(
            json!({ "type": "auto_compaction_start", "reason": "threshold", "action": "context-full" }),
        ),
        message_end("m1", user_message("hi")),
        omp(
            json!({ "type": "auto_compaction_end", "action": "context-full", "aborted": false, "willRetry": false }),
        ),
        omp(json!({ "type": "manual_compaction_start" })),
        omp(json!({ "type": "manual_compaction_end", "aborted": true })),
        omp(
            json!({ "type": "auto_retry_start", "attempt": 1, "maxAttempts": 3, "delayMs": 10, "errorMessage": "429" }),
        ),
        omp(json!({ "type": "auto_retry_end", "success": true, "attempt": 1 })),
        omp(json!({ "type": "notice", "level": "info", "message": "saved" })),
        omp(
            json!({ "type": "extension_error", "extensionPath": "/x.ts", "event": "e", "error": "bad" }),
        ),
        omp(
            json!({ "type": "rpc_frame_error", "error": "too big", "originalType": "message_update" }),
        ),
        omp(json!({ "type": "brand_new_frame", "x": 1 })),
        event(json!({ "kind": "unknown", "raw": { "weird": true } })),
    ]);
    let kinds: Vec<&str> = state
        .notices
        .iter()
        .map(|notice| match notice.frame {
            NoticeFrame::AutoCompactionStart(_) => "auto_compaction_start",
            NoticeFrame::AutoCompactionEnd(_) => "auto_compaction_end",
            NoticeFrame::ManualCompactionStart(_) => "manual_compaction_start",
            NoticeFrame::ManualCompactionEnd(_) => "manual_compaction_end",
            NoticeFrame::AutoRetryStart(_) => "auto_retry_start",
            NoticeFrame::AutoRetryEnd(_) => "auto_retry_end",
            NoticeFrame::Notice(_) => "notice",
            NoticeFrame::ExtensionError(_) => "extension_error",
            NoticeFrame::RpcFrameError(_) => "rpc_frame_error",
            NoticeFrame::Unknown(_) => "unknown",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "auto_compaction_start",
            "auto_compaction_end",
            "manual_compaction_start",
            "manual_compaction_end",
            "auto_retry_start",
            "auto_retry_end",
            "notice",
            "extension_error",
            "rpc_frame_error",
            "unknown",
            "unknown",
        ]
    );
    let positions: Vec<usize> = state.notices.iter().map(|notice| notice.position).collect();
    assert_eq!(positions, [0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1]);
    assert_eq!(
        state.notices[9].frame,
        NoticeFrame::Unknown(json!({ "type": "brand_new_frame", "x": 1 }))
    );
    assert_eq!(
        state.notices[10].frame,
        NoticeFrame::Unknown(json!({ "weird": true }))
    );
}

// spec: TST-010
#[test]
fn history_drops_notices_placed_in_the_previous_transcript() {
    let state = replay(&[
        omp(json!({ "type": "notice", "level": "info", "message": "old" })),
        history(json!([])),
    ]);
    assert_eq!(state.notices, Vec::new());
}

// spec: TST-010, PRT-030
#[test]
fn history_drops_prompt_results_of_the_previous_process() {
    // omp numbers requests `req_N` per process, so ids restart after a restart (a new History).
    let state = replay(&[
        omp(
            json!({ "type": "prompt_result", "id": "req_5", "agentInvoked": true, "status": "completed", "sessionSettled": true }),
        ),
        history(json!([])),
    ]);
    assert_eq!(state.prompts.len(), 0);
}

// spec: TST-010
#[test]
fn frames_the_reducer_does_not_need_leave_the_state_unchanged() {
    let base = replay(&[
        process(json!({ "state": "running" })),
        message_end("m1", user_message("hi")),
    ]);
    for frame in [
        json!({ "type": "ready", "protocolVersion": 2 }),
        json!({ "type": "advisor_cost_changed" }),
        json!({ "type": "turn_start" }),
        json!({ "type": "live_levels", "input": 0.1, "output": 0.2 }),
        json!({ "type": "live_phase", "phase": "listening" }),
        json!({ "type": "dictation_transcript", "captureId": "c", "text": "t" }),
        json!({ "type": "model_changed" }),
        json!({ "type": "btw_delta", "recordId": "r", "delta": "d" }),
        json!({ "type": "message_update", "messageId": "m2", "message": { "role": "assistant" }, "assistantMessageEvent": { "type": "done", "reason": "stop" } }),
    ] {
        let mut state = base.clone();
        state.apply(&omp(frame.clone()));
        assert_eq!(state, base, "{frame}");
    }
}
