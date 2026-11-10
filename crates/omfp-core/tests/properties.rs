//! Property tests of the reducer (TST-011). The snapshot property (b) runs exhaustively over every fixture and split
//! point in `fixtures.rs`.

mod common;

use std::collections::BTreeMap;

use common::{event, omp};
use omfp_core::wire::{AgentMessage, AssistantContent, AssistantMessage};
use omfp_core::{QueueState, ThreadEvent, ThreadState};
use proptest::prelude::*;
use serde_json::{Value, json};

/// One generated content block of an assistant message.
#[derive(Debug, Clone)]
enum Block {
    Text(String),
    Thinking {
        thinking: String,
        signature: Option<String>,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: BTreeMap<String, String>,
    },
}

impl Block {
    fn to_json(&self) -> Value {
        match self {
            Self::Text(text) => json!({ "type": "text", "text": text }),
            Self::Thinking {
                thinking,
                signature: Some(signature),
            } => {
                json!({ "type": "thinking", "thinking": thinking, "thinkingSignature": signature })
            }
            Self::Thinking {
                thinking,
                signature: None,
            } => json!({ "type": "thinking", "thinking": thinking }),
            Self::ToolCall {
                id,
                name,
                arguments,
            } => {
                json!({ "type": "toolCall", "id": id, "name": name, "arguments": arguments })
            }
        }
    }

    /// The text the deltas of this block carry, in order.
    fn streamed(&self) -> String {
        match self {
            Self::Text(text) | Self::Thinking { thinking: text, .. } => text.clone(),
            Self::ToolCall { arguments, .. } => {
                serde_json::to_string(arguments).expect("serializable arguments")
            }
        }
    }
}

fn block() -> impl Strategy<Value = Block> {
    prop_oneof![
        "\\PC{0,40}".prop_map(Block::Text),
        ("\\PC{0,40}", proptest::option::of("[a-zA-Z0-9+/]{1,12}")).prop_map(
            |(thinking, signature)| Block::Thinking {
                thinking,
                signature
            }
        ),
        (
            "[a-z0-9_]{1,10}",
            "[a-z_]{1,8}",
            prop::collection::btree_map("[a-z]{1,6}", "\\PC{0,20}", 0..4)
        )
            .prop_map(|(id, name, arguments)| Block::ToolCall {
                id,
                name,
                arguments
            }),
    ]
}

/// Splits `text` into pieces at the char boundaries chosen by `cuts`.
fn split(text: &str, cuts: &[prop::sample::Index]) -> Vec<String> {
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(index, _)| index)
        .chain([text.len()])
        .collect();
    let mut points: Vec<usize> = cuts
        .iter()
        .map(|cut| boundaries[cut.index(boundaries.len())])
        .collect();
    points.extend([0, text.len()]);
    points.sort_unstable();
    points.dedup();
    points
        .windows(2)
        .map(|pair| text[pair[0]..pair[1]].to_owned())
        .collect()
}

fn update(assistant_event: Value) -> ThreadEvent {
    let mut frame = json!({
        "type": "message_update",
        "messageId": "msg-1",
        "message": { "role": "assistant" },
    });
    frame["assistantMessageEvent"] = assistant_event;
    omp(frame)
}

fn start_event(index: usize, block: &Block) -> ThreadEvent {
    let (kind, shell) = match block {
        Block::Text(_) => ("text_start", json!({ "type": "text", "text": "" })),
        Block::Thinking { .. } => (
            "thinking_start",
            json!({ "type": "thinking", "thinking": "" }),
        ),
        Block::ToolCall { id, name, .. } => (
            "toolcall_start",
            json!({ "type": "toolCall", "id": id, "name": name, "arguments": {} }),
        ),
    };
    update(json!({ "type": kind, "contentIndex": index, "block": shell }))
}

fn delta_event(index: usize, block: &Block, delta: &str) -> ThreadEvent {
    let kind = match block {
        Block::Text(_) => "text_delta",
        Block::Thinking { .. } => "thinking_delta",
        Block::ToolCall { .. } => "toolcall_delta",
    };
    update(json!({ "type": kind, "contentIndex": index, "delta": delta }))
}

fn end_event(index: usize, block: &Block) -> ThreadEvent {
    match block {
        Block::Text(text) => update(
            json!({ "type": "text_end", "contentIndex": index, "content": text, "block": { "type": "text" } }),
        ),
        Block::Thinking {
            thinking,
            signature,
        } => {
            let mut shell = json!({ "type": "thinking" });
            if let (Some(signature), Some(shell)) = (signature, shell.as_object_mut()) {
                shell.insert("thinkingSignature".into(), json!(signature));
            }
            update(
                json!({ "type": "thinking_end", "contentIndex": index, "content": thinking, "block": shell }),
            )
        }
        Block::ToolCall { .. } => update(
            json!({ "type": "toolcall_end", "contentIndex": index, "toolCall": block.to_json() }),
        ),
    }
}

/// The streamed string of the rebuilt block at `index`: text, thinking, or a tool call's argument JSON so far.
fn rebuilt(state: &ThreadState, index: usize) -> Option<String> {
    match state
        .streaming
        .as_ref()?
        .message
        .content
        .as_ref()?
        .get(index)?
    {
        AssistantContent::Text(text) => text.text.clone(),
        AssistantContent::Thinking(thinking) => thinking.thinking.clone(),
        AssistantContent::ToolCall(call) => call
            .extra
            .get("partialArgs")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

proptest! {
    // spec: TST-011
    #[test]
    fn delta_updates_rebuild_the_message_end_content(
        blocks in prop::collection::vec(
            (block(), prop::collection::vec(any::<prop::sample::Index>(), 0..6)),
            0..5,
        ),
        snapshot_ahead in any::<bool>(),
    ) {
        let content: Vec<Value> = blocks.iter().map(|(block, _)| block.to_json()).collect();
        let final_message = json!({
            "role": "assistant",
            "content": content,
            "stopReason": "stop",
            "responseId": "r-1",
            "timestamp": 0,
        });
        // omp snapshots `message_start` when it handles it, so it may already hold content the deltas deliver again.
        let start_content = if snapshot_ahead { json!(content) } else { json!([]) };
        let mut state = ThreadState::default();
        state.apply(&omp(json!({
            "type": "message_start",
            "messageId": "msg-1",
            "message": { "role": "assistant", "content": start_content, "timestamp": 0 },
        })));

        for (index, (block, cuts)) in blocks.iter().enumerate() {
            state.apply(&start_event(index, block));
            prop_assert_eq!(rebuilt(&state, index), Some(String::new()));
            let streamed = block.streamed();
            for piece in split(&streamed, cuts) {
                state.apply(&delta_event(index, block, &piece));
            }
            prop_assert_eq!(rebuilt(&state, index), Some(streamed));
            state.apply(&end_event(index, block));
        }

        let expected: AssistantMessage = serde_json::from_value(final_message.clone()).expect("valid message");
        let streaming = state.streaming.clone().expect("streaming message");
        prop_assert_eq!(streaming.message_id.as_deref(), Some("msg-1"));
        prop_assert_eq!(&streaming.message.content, &expected.content);

        state.apply(&omp(json!({ "type": "message_end", "messageId": "msg-1", "message": final_message })));
        prop_assert_eq!(&state.streaming, &None);
        prop_assert_eq!(state.transcript.len(), 1);
        prop_assert_eq!(&state.transcript[0].message, &AgentMessage::Assistant(expected));
    }
}

/// Events that must not touch the queue, mixed between `queue_update`s.
fn other_events() -> Vec<ThreadEvent> {
    vec![
        omp(json!({ "type": "agent_start" })),
        omp(json!({ "type": "session_settled" })),
        omp(json!({ "type": "agent_end", "messages": [] })),
        omp(
            json!({ "type": "message_end", "messageId": "m", "message": { "role": "user", "content": "hi" } }),
        ),
        omp(
            json!({ "type": "extension_ui_request", "id": "s", "method": "select", "title": "t", "options": ["a"] }),
        ),
        event(json!({ "kind": "process", "state": { "state": "running" } })),
        event(json!({ "kind": "process", "state": { "state": "failed", "message": "exit" } })),
        event(json!({ "kind": "process", "state": { "state": "stopped" } })),
        event(
            json!({ "kind": "history", "session": { "session_id": "s", "resumed": true }, "messages": [] }),
        ),
        event(json!({ "kind": "unknown", "raw": { "type": "queue_update" } })),
    ]
}

#[derive(Debug, Clone)]
enum Step {
    Queue(QueueState),
    Other(prop::sample::Index),
}

fn queue_state() -> impl Strategy<Value = QueueState> {
    (
        prop::collection::vec("\\PC{0,12}", 0..4),
        prop::collection::vec("\\PC{0,12}", 0..4),
        0..3_i64,
    )
        .prop_map(|(steering, follow_up, live_steered)| QueueState {
            steering,
            follow_up,
            live_steered,
        })
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        queue_state().prop_map(Step::Queue),
        any::<prop::sample::Index>().prop_map(Step::Other)
    ]
}

proptest! {
    // spec: TST-011
    #[test]
    fn queue_always_equals_the_last_queue_update(steps in prop::collection::vec(step(), 0..40)) {
        let others = other_events();
        let mut state = ThreadState::default();
        let mut last = QueueState::default();
        for step in steps {
            match step {
                Step::Queue(queue) => {
                    state.apply(&omp(json!({
                        "type": "queue_update",
                        "steering": queue.steering,
                        "followUp": queue.follow_up,
                        "liveSteered": queue.live_steered,
                    })));
                    last = queue;
                }
                Step::Other(index) => state.apply(index.get(&others)),
            }
            prop_assert_eq!(&state.queue, &last);
        }
    }
}
