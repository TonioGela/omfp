//! The thread view: header, transcript (Markdown, thinking, notices, tool cards), virtualization.

use std::collections::BTreeMap;

use omfp_core::wire::{AgentMessage, AssistantContent, SessionState, ThinkingLevel};
use omfp_core::{Notice, NoticeFrame, ProcessState, ThreadState, TranscriptEntry};
use serde_json::json;

use super::{Page, attr, index, normalize, render, selecting, text, with_open};
use crate::test_support::{fixture, fixture_events};
use crate::{ThreadDetails, ThreadView, UiState};

/// `state` with its assistant replies' text replaced by `text`.
fn with_reply(mut state: ThreadState, reply: &str) -> ThreadState {
    for entry in &mut state.transcript {
        if let AgentMessage::Assistant(message) = &mut entry.message {
            for block in message.content.iter_mut().flatten() {
                if let AssistantContent::Text(text) = block {
                    text.text = Some(reply.to_owned());
                }
            }
        }
    }
    state
}

fn details() -> ThreadDetails {
    let session: SessionState = serde_json::from_value(json!({
        "sessionId": "s",
        "model": {
            "id": "scripted", "name": "Scripted", "api": "openai-completions", "provider": "fake", "baseUrl": "",
            "reasoning": false, "input": ["text"], "contextWindow": 128_000, "maxTokens": 4096,
            "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}
        },
        "thinkingLevel": "low",
        "contextUsage": {"tokens": 3200, "contextWindow": 128_000, "percent": 2.5}
    }))
    .expect("session state");
    ThreadDetails {
        session: Some(session),
        models: Vec::new(),
        thinking_levels: vec![ThinkingLevel::Off, ThinkingLevel::Low],
    }
}

// spec: UI-030
#[test]
fn the_header_shows_title_tags_chips_session_and_process_controls() {
    let mut input = with_open(index(), "t-checkout", fixture("text_reply"));
    if let Some(open) = input.open.get_mut("t-checkout") {
        open.details = details();
        if let Some(AgentMessage::Assistant(reply)) = open
            .state
            .transcript
            .get_mut(1)
            .map(|entry| &mut entry.message)
        {
            reply
                .usage
                .get_or_insert_default()
                .cost
                .get_or_insert_default()
                .total = Some(0.0042);
        }
    }
    let page = Page::new(input, selecting("t-checkout"));
    assert_eq!(text(page.one("thread-title")), "checkout-v2");
    let tags: Vec<String> = page.testid("thread-tag").into_iter().map(text).collect();
    assert_eq!(tags, vec!["release"]);
    page.one("edit-header");
    let chips = page.all("[data-testid=\"thread-header\"] [data-testid=\"workspace-chip\"]");
    assert_eq!(
        chips
            .iter()
            .map(|chip| (text(*chip), attr(*chip, "data-kind")))
            .collect::<Vec<_>>(),
        vec![
            ("⌂checkout".to_owned(), Some("workspace")),
            ("fe:feat/checkout".to_owned(), Some("worktree")),
            ("be:main".to_owned(), Some("link"))
        ]
    );
    assert_eq!(
        attr(page.one("model-select"), "value"),
        Some("fake/scripted")
    );
    assert_eq!(attr(page.one("thinking-select"), "value"), Some("low"));
    assert_eq!(text(page.one("context-usage")), "2% of 128k");
    assert_eq!(text(page.one("session-cost")), "$0.0042");
    assert_eq!(
        attr(page.one("thread-state"), "data-status"),
        Some("awaiting")
    );
    assert_eq!(text(page.one("process-action")), "Stop");
}

// spec: UI-030
#[test]
fn a_failed_thread_shows_why_and_offers_a_restart_and_the_title_editor() {
    let mut ui = selecting("t-scratch");
    ui.header_edit = Some(crate::local::HeaderEdit {
        title: "scratch".to_owned(),
        tags: "a, b".to_owned(),
    });
    let page = Page::new(with_open(index(), "t-scratch", ThreadState::default()), ui);
    assert_eq!(text(page.one("thread-failure")), "omp exited");
    assert_eq!(text(page.one("process-action")), "Restart");
    assert_eq!(attr(page.one("title-input"), "value"), Some("scratch"));
    assert_eq!(attr(page.one("tags-input"), "value"), Some("a, b"));
}

// spec: UI-040
#[test]
fn assistant_text_renders_as_sanitized_markdown() {
    let state = with_reply(
        fixture("text_reply"),
        "**Done**: see [docs](https://example.test).\n\n<script>alert(1)</script><img src=x onerror=alert(2)>",
    );
    let html = render(
        with_open(index(), "t-checkout", state),
        selecting("t-checkout"),
    );
    let page = scraper::Html::parse_fragment(&html);
    let markdown = scraper::Selector::parse("[data-testid=\"markdown\"]").expect("selector");
    let reply = page
        .select(&markdown)
        .next()
        .expect("markdown block")
        .inner_html();
    assert!(reply.contains("<strong>Done</strong>"), "{reply}");
    assert!(reply.contains("href=\"https://example.test\""), "{reply}");
    assert!(!html.contains("<script"), "{html}");
    assert!(!html.contains("onerror"), "{html}");
    let user = scraper::Selector::parse("[data-testid=\"plain-text\"]").expect("selector");
    assert_eq!(page.select(&user).count(), 1, "user text stays plain");
}

fn thinking_state() -> ThreadState {
    let message: AgentMessage = serde_json::from_value(json!({
        "role": "assistant",
        "content": [{"type": "thinking", "thinking": "weighing options"}, {"type": "text", "text": "Answer"}]
    }))
    .expect("message");
    ThreadState {
        transcript: vec![TranscriptEntry {
            message_id: None,
            message,
        }],
        ..ThreadState::default()
    }
}

// spec: UI-041, UI-010
#[test]
fn thinking_is_collapsed_by_default_and_expands_from_window_state() {
    let input = with_open(index(), "t-checkout", thinking_state());
    let collapsed = Page::new(input.clone(), selecting("t-checkout"));
    let toggle = collapsed.all("[data-testid=\"thinking\"] [data-testid=\"toggle\"]");
    assert_eq!(attr(toggle[0], "aria-expanded"), Some("false"));
    assert_eq!(collapsed.all(".thinking-text").len(), 0);
    let mut ui = selecting("t-checkout");
    ui.expanded.insert("t-checkout/m0/0".to_owned(), true);
    let expanded = Page::new(input, ui);
    assert_eq!(
        attr(
            expanded.all("[data-testid=\"thinking\"] [data-testid=\"toggle\"]")[0],
            "aria-expanded"
        ),
        Some("true")
    );
    assert_eq!(text(expanded.all(".thinking-text")[0]), "weighing options");
}

// spec: UI-042
#[test]
fn a_long_transcript_renders_only_its_tail_with_a_spacer_for_the_rest() {
    let mut state = fixture("text_reply");
    let entry = state.transcript[1].clone();
    state.transcript = std::iter::repeat_n(entry, 400).collect();
    let page = Page::new(
        with_open(index(), "t-checkout", state),
        selecting("t-checkout"),
    );
    let items = page.testid("transcript-item");
    assert_eq!(items.len(), 25);
    assert_eq!(
        attr(*items.last().expect("an item"), "data-key"),
        Some("m399")
    );
    assert_eq!(
        attr(page.one("spacer-top"), "style"),
        Some("height: 45000px")
    );
    assert_eq!(
        attr(page.one("spacer-bottom"), "style"),
        Some("height: 0px")
    );
}

// spec: UI-042
#[test]
fn the_transcript_container_belongs_to_the_selected_thread() {
    let input = with_open(
        with_open(index(), "t-checkout", fixture("text_reply")),
        "t-idle",
        fixture("text_reply"),
    );
    for thread_id in ["t-checkout", "t-idle"] {
        let page = Page::new(input.clone(), selecting(thread_id));
        let transcript = page.one("transcript");
        assert_eq!(
            (
                attr(transcript, "data-thread-id"),
                attr(transcript, "data-pinned")
            ),
            (Some(thread_id), Some("true"))
        );
    }
}

// spec: UI-042
#[test]
fn the_streaming_reply_is_rendered_last_and_marked_busy() {
    let events = fixture_events("text_reply");
    let end = events
        .iter()
        .position(|event| {
            serde_json::to_value(event)
                .ok()
                .and_then(|value| value.pointer("/frame/type").cloned())
                == Some(json!("message_end"))
                && serde_json::to_value(event)
                    .ok()
                    .and_then(|value| value.pointer("/frame/message/role").cloned())
                    == Some(json!("assistant"))
        })
        .expect("assistant message_end");
    let state = omfp_core::replay(&events[..end]);
    let page = Page::new(
        with_open(index(), "t-checkout", state),
        selecting("t-checkout"),
    );
    let messages = page.testid("message");
    let last = *messages.last().expect("a message");
    assert_eq!(
        (attr(last, "data-streaming"), attr(last, "aria-busy")),
        (Some("true"), Some("true"))
    );
    assert_ne!(text(last), "");
}

// spec: UI-043
#[test]
fn notices_appear_inline_and_unknown_kinds_expand_to_json() {
    let mut state = fixture("text_reply");
    state.notices = vec![
        Notice {
            position: 1,
            frame: serde_json::from_value(json!({
                "type": "auto_retry_start",
                "data": {"attempt": 1, "maxAttempts": 3, "delayMs": 2000, "errorMessage": "overloaded"}
            }))
            .expect("retry"),
        },
        Notice { position: 2, frame: NoticeFrame::Unknown(json!({"type": "irc_card", "from": "advisor"})) },
    ];
    let input = with_open(index(), "t-checkout", state);
    let page = Page::new(input.clone(), selecting("t-checkout"));
    let keys: Vec<&str> = page
        .testid("transcript-item")
        .into_iter()
        .filter_map(|item| attr(item, "data-key"))
        .collect();
    assert_eq!(keys, vec!["m0", "n0", "m1", "n1"]);
    let notices = page.testid("notice");
    assert_eq!(
        notices
            .iter()
            .map(|notice| (attr(*notice, "data-kind"), attr(*notice, "data-level")))
            .collect::<Vec<_>>(),
        vec![
            (Some("retry"), Some("warning")),
            (Some("unknown"), Some("info"))
        ]
    );
    assert_eq!(page.testid("notice-json").len(), 0);
    let mut ui = selecting("t-checkout");
    ui.expanded.insert("t-checkout/n1".to_owned(), true);
    let expanded = Page::new(input, ui);
    assert!(text(expanded.one("notice-json")).contains("\"from\": \"advisor\""));
}

// spec: UI-050, UI-052
#[test]
fn tools_render_through_the_collapsible_fallback_card() {
    let input = with_open(index(), "t-checkout", fixture("bash_tool_call"));
    let collapsed = Page::new(input.clone(), selecting("t-checkout"));
    let card = collapsed.one("tool-card");
    assert_eq!(
        (attr(card, "data-tool"), attr(card, "data-status")),
        (Some("bash"), Some("succeeded"))
    );
    assert_eq!(
        text(collapsed.one("tool-summary")),
        "command: echo tool-output-123"
    );
    assert_eq!(text(collapsed.one("tool-status")), "done");
    assert_eq!(collapsed.testid("tool-args").len(), 0);
    let mut ui = selecting("t-checkout");
    ui.expanded.insert("t-checkout/m1/0".to_owned(), true);
    let expanded = Page::new(input, ui);
    assert!(text(expanded.one("tool-args")).contains("\"command\": \"echo tool-output-123\""));
    assert_eq!(text(expanded.one("tool-result")), "tool-output-123");
}

// spec: UI-052
#[test]
fn failed_tools_are_marked_and_expanded_by_default() {
    let mut state = fixture("bash_tool_call");
    for run in &mut state.tools {
        run.status = omfp_core::ToolStatus::Failed;
        run.result = Some(json!({"content": [{"type": "text", "text": "exit 1"}]}));
    }
    let page = Page::new(
        with_open(index(), "t-checkout", state),
        selecting("t-checkout"),
    );
    assert_eq!(attr(page.one("tool-card"), "data-status"), Some("failed"));
    assert_eq!(text(page.one("tool-status")), "failed");
    assert_eq!(text(page.one("tool-result")), "exit 1");
}

// spec: UI-020
#[test]
fn an_unsubscribed_thread_shows_its_header_while_loading() {
    let page = Page::new(index(), selecting("t-flaky"));
    page.one("thread-header");
    page.one("thread-loading");
    assert_eq!(page.testid("composer").len(), 0);
    let none = Page::new(index(), UiState::default());
    none.one("no-thread");
}

// spec: TST-031, TST-032
#[test]
fn thread_snapshot_with_a_tool_call() {
    let mut input = index();
    input.open.insert(
        "t-checkout".to_owned(),
        ThreadView {
            state: fixture("bash_tool_call"),
            details: details(),
            ..ThreadView::default()
        },
    );
    let mut ui = selecting("t-checkout");
    ui.expanded = BTreeMap::from([("t-checkout/m1/0".to_owned(), true)]);
    insta::assert_snapshot!(normalize(&render(input, ui)));
}

// spec: UI-020
#[test]
fn a_starting_thread_is_badged_as_starting() {
    let mut input = index();
    input.threads[3].process = ProcessState::Starting;
    let page = Page::new(input, UiState::default());
    assert_eq!(
        attr(page.all("[data-thread-id=\"t-idle\"]")[0], "data-status"),
        Some("starting")
    );
}
