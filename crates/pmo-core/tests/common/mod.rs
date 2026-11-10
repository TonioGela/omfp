//! Helpers shared by the reducer tests.
#![allow(dead_code)] // Each test crate uses a subset.

use std::path::PathBuf;

use pmo_core::wire::{AgentMessage, AssistantContent, AssistantMessage, UserContent};
use pmo_core::{ThreadEvent, ThreadState, replay};
use serde_json::{Value, json};

/// Names of every recorded thread fixture.
pub const FIXTURES: [&str; 6] = [
    "approval_granted",
    "ask_answered",
    "bash_tool_call",
    "follow_up_then_abort",
    "restart_with_history",
    "text_reply",
];

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/threads")
        .join(pmo_core::OMP_VERSION)
}

/// The recorded reducer inputs of fixture `name`.
pub fn fixture(name: &str) -> Vec<ThreadEvent> {
    let path = fixture_dir().join(format!("{name}.jsonl"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("{}: {error}: {line}", path.display()))
        })
        .collect()
}

/// A `ThreadEvent` from its JSON form.
pub fn event(value: Value) -> ThreadEvent {
    serde_json::from_value(value).expect("valid ThreadEvent")
}

/// A `ThreadEvent::Omp` wrapping the omp frame `frame`.
pub fn omp(frame: Value) -> ThreadEvent {
    let mut value = json!({ "kind": "omp" });
    value["frame"] = frame;
    event(value)
}

/// Index of the `nth` (0-based) event whose `kind`, frame `type` or assistant event `type` equals `ty`.
pub fn index_of(events: &[ThreadEvent], ty: &str, nth: usize) -> usize {
    events
        .iter()
        .enumerate()
        .filter(|(_, event)| {
            let value = serde_json::to_value(event).expect("serializable event");
            [
                value.get("kind"),
                value.pointer("/frame/type"),
                value.pointer("/frame/assistantMessageEvent/type"),
            ]
            .into_iter()
            .flatten()
            .any(|found| found == ty)
        })
        .nth(nth)
        .map_or_else(
            || panic!("no event #{nth} of type {ty}"),
            |(index, _)| index,
        )
}

/// State after applying `events[..=index]`.
pub fn state_through(events: &[ThreadEvent], index: usize) -> ThreadState {
    replay(&events[..=index])
}

/// Concatenated text blocks of an assistant message.
pub fn assistant_text(message: &AssistantMessage) -> String {
    message
        .content
        .iter()
        .flatten()
        .filter_map(|block| match block {
            AssistantContent::Text(text) => text.text.clone(),
            _ => None,
        })
        .collect()
}

/// Concatenated text of any user or assistant message.
pub fn message_text(message: &AgentMessage) -> String {
    match message {
        AgentMessage::Assistant(assistant) => assistant_text(assistant),
        AgentMessage::User(user) => match &user.content {
            Some(pmo_core::wire::MessageContent::String(text)) => text.clone(),
            Some(pmo_core::wire::MessageContent::UserContentList(blocks)) => blocks
                .iter()
                .filter_map(|block| match block {
                    UserContent::Text(text) => text.text.clone(),
                    UserContent::Image(_) => None,
                })
                .collect(),
            None => String::new(),
        },
        _ => String::new(),
    }
}

/// Text of the streaming message, or `None` when nothing streams.
pub fn streaming_text(state: &ThreadState) -> Option<String> {
    state
        .streaming
        .as_ref()
        .map(|streaming| assistant_text(&streaming.message))
}
