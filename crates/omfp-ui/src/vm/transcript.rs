//! The transcript as a flat list of items (messages, inline notices, the streaming reply) and the display blocks of
//! each message. Items only hold indices into the thread state, so building the list stays cheap for long transcripts
//! and the virtualized list (UI-042) renders the visible ones only.

use std::collections::HashSet;

use omfp_core::ThreadState;
use omfp_core::wire::{
    AgentMessage, AssistantContent, AssistantMessage, MessageContent, UserContent,
};
use serde_json::Value;

use super::tool::{ToolCard, tool_card};
use super::{one_line, pretty_json};

/// What an item renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// `state.transcript[index]`.
    Message(usize),
    /// `state.notices[index]`.
    Notice(usize),
    /// `state.streaming`.
    Streaming,
}

/// One row of the transcript with a key that stays stable while the transcript grows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    pub kind: ItemKind,
}

/// The transcript's rows in order: each notice goes after the messages that preceded it, tool results shown inside
/// their call's card are left out, and the streaming reply comes last.
#[must_use]
pub fn items(state: &ThreadState) -> Vec<Item> {
    let called: HashSet<&str> = state
        .transcript
        .iter()
        .filter_map(|entry| match &entry.message {
            AgentMessage::Assistant(message) => Some(message),
            _ => None,
        })
        .flat_map(|message| message.content.iter().flatten())
        .filter_map(|block| match block {
            AssistantContent::ToolCall(call) => call.id.as_deref(),
            _ => None,
        })
        .collect();
    let mut items = Vec::with_capacity(state.transcript.len() + state.notices.len() + 1);
    let mut notices = state.notices.iter().enumerate().peekable();
    for (index, entry) in state.transcript.iter().enumerate() {
        while let Some((notice, _)) = notices.next_if(|(_, notice)| notice.position <= index) {
            items.push(Item {
                key: format!("n{notice}"),
                kind: ItemKind::Notice(notice),
            });
        }
        if let AgentMessage::ToolResult(result) = &entry.message
            && result
                .tool_call_id
                .as_deref()
                .is_some_and(|id| called.contains(id))
        {
            continue;
        }
        items.push(Item {
            key: format!("m{index}"),
            kind: ItemKind::Message(index),
        });
    }
    items.extend(notices.map(|(notice, _)| Item {
        key: format!("n{notice}"),
        kind: ItemKind::Notice(notice),
    }));
    if state.streaming.is_some() {
        items.push(Item {
            key: "streaming".to_owned(),
            kind: ItemKind::Streaming,
        });
    }
    items
}

/// Who wrote a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    /// Tool results without a visible call, bash executions, summaries, custom records, …
    Record,
}

impl Role {
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Record => "record",
        }
    }
}

/// One displayed part of a message.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// Markdown for the assistant (UI-040), plain text for everyone else.
    Text(String),
    /// Collapsed by default (UI-041).
    Thinking(String),
    Image {
        mime: String,
        data: String,
    },
    Tool(ToolCard),
    /// A message kind without its own renderer: a one-line summary and its JSON.
    Record {
        summary: String,
        json: String,
    },
}

/// A message ready to render.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageView {
    pub role: Role,
    /// `You`, `Assistant`, or the record kind.
    pub label: String,
    pub blocks: Vec<Block>,
    /// The provider's error when the reply failed.
    pub error: Option<String>,
    /// A user message sent as steering.
    pub steering: bool,
}

/// The view of `state.transcript[index]`.
pub fn message_view(state: &ThreadState, index: usize) -> MessageView {
    match state.transcript.get(index).map(|entry| &entry.message) {
        Some(AgentMessage::User(user)) => MessageView {
            role: Role::User,
            label: "You".to_owned(),
            blocks: user
                .content
                .as_ref()
                .map(content_blocks)
                .unwrap_or_default(),
            error: None,
            steering: user.steering == Some(true),
        },
        Some(AgentMessage::Assistant(message)) => assistant_view(state, message),
        Some(other) => record_view(other),
        None => MessageView {
            role: Role::Record,
            label: "missing".to_owned(),
            blocks: Vec::new(),
            error: None,
            steering: false,
        },
    }
}

/// The view of the reply being streamed, if any.
#[must_use]
pub fn streaming_view(state: &ThreadState) -> Option<MessageView> {
    state
        .streaming
        .as_ref()
        .map(|streaming| assistant_view(state, &streaming.message))
}

fn assistant_view(state: &ThreadState, message: &AssistantMessage) -> MessageView {
    let called_at = message.completed_at.or(message.timestamp);
    let blocks = message
        .content
        .iter()
        .flatten()
        .filter_map(|block| match block {
            AssistantContent::Text(text) => {
                Some(Block::Text(text.text.clone().unwrap_or_default()))
            }
            AssistantContent::Thinking(thinking) if thinking.narration == Some(true) => {
                Some(Block::Text(thinking.thinking.clone().unwrap_or_default()))
            }
            AssistantContent::Thinking(thinking) => Some(Block::Thinking(
                thinking.thinking.clone().unwrap_or_default(),
            )),
            AssistantContent::RedactedThinking(_) => {
                Some(Block::Thinking("(redacted by the provider)".to_owned()))
            }
            AssistantContent::Image(image) => Some(Block::Image {
                mime: image
                    .mime_type
                    .clone()
                    .unwrap_or_else(|| "image/png".to_owned()),
                data: image.data.clone().unwrap_or_default(),
            }),
            AssistantContent::ToolCall(call) => {
                let id = call.id.clone().unwrap_or_default();
                let run = state.tools.iter().find(|run| run.tool_call_id == id);
                let result = state
                    .transcript
                    .iter()
                    .find_map(|entry| match &entry.message {
                        AgentMessage::ToolResult(result)
                            if result.tool_call_id.as_deref() == Some(id.as_str()) =>
                        {
                            Some(result)
                        }
                        _ => None,
                    });
                Some(Block::Tool(tool_card(
                    &id,
                    Some(call),
                    run,
                    result,
                    called_at,
                )))
            }
            AssistantContent::Fallback(_) | AssistantContent::AnthropicServerTool(_) => None,
        })
        .collect();
    MessageView {
        role: Role::Assistant,
        label: "Assistant".to_owned(),
        blocks,
        error: message.error_message.clone(),
        steering: false,
    }
}

fn content_blocks(content: &MessageContent) -> Vec<Block> {
    match content {
        MessageContent::String(text) => vec![Block::Text(text.clone())],
        MessageContent::UserContentList(parts) => parts
            .iter()
            .map(|part| match part {
                UserContent::Text(text) => Block::Text(text.text.clone().unwrap_or_default()),
                UserContent::Image(image) => Block::Image {
                    mime: image
                        .mime_type
                        .clone()
                        .unwrap_or_else(|| "image/png".to_owned()),
                    data: image.data.clone().unwrap_or_default(),
                },
            })
            .collect(),
    }
}

/// A generic row for message kinds without their own renderer: kind label, one-line summary, JSON.
fn record_view(message: &AgentMessage) -> MessageView {
    let value = serde_json::to_value(message).unwrap_or(Value::Null);
    let role = value
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("record");
    let label = record_label(role).to_owned();
    let summary = ["content", "summary", "command", "code", "output", "text"]
        .iter()
        .find_map(|key| value.get(*key).map(text_of))
        .unwrap_or_default();
    MessageView {
        role: Role::Record,
        label,
        blocks: vec![Block::Record {
            summary: one_line(&summary, 200),
            json: pretty_json(&value),
        }],
        error: None,
        steering: false,
    }
}

fn record_label(role: &str) -> &str {
    match role {
        "toolResult" => "tool result",
        "bashExecution" => "bash",
        "pythonExecution" => "python",
        "hookMessage" => "hook",
        "branchSummary" => "branch summary",
        "compactionSummary" => "compaction summary",
        "fileMention" => "file mention",
        other => other,
    }
}

/// A string, or the text parts of a content list, or compact JSON.
fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" "),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use omfp_core::{Notice, NoticeFrame};
    use serde_json::json;

    use super::*;
    use crate::test_support::{fixture, fixture_until};

    fn kinds(state: &ThreadState) -> Vec<String> {
        items(state)
            .into_iter()
            .map(|item| match item.kind {
                ItemKind::Message(index) => message_view(state, index).role.token().to_owned(),
                ItemKind::Notice(_) => "notice".to_owned(),
                ItemKind::Streaming => "streaming".to_owned(),
            })
            .collect()
    }

    // spec: UI-040
    #[test]
    fn a_text_reply_is_a_user_message_and_an_assistant_text() {
        let state = fixture("text_reply");
        assert_eq!(kinds(&state), vec!["user", "assistant"]);
        let reply = message_view(&state, 1);
        assert!(
            matches!(reply.blocks.as_slice(), [Block::Text(text)] if !text.is_empty()),
            "{reply:?}"
        );
    }

    // spec: UI-050
    #[test]
    fn tool_results_shown_in_their_card_are_not_listed_again() {
        let state = fixture("bash_tool_call");
        assert_eq!(state.transcript.len(), 4);
        assert_eq!(kinds(&state), vec!["user", "assistant", "assistant"]);
    }

    // spec: UI-042
    #[test]
    fn the_streaming_reply_is_the_last_item_with_its_text_so_far() {
        let state = fixture_until("text_reply", |event| {
            event
                .pointer("/frame/assistantMessageEvent/type")
                .and_then(Value::as_str)
                == Some("text_end")
        });
        assert_eq!(kinds(&state).last().map(String::as_str), Some("streaming"));
        let view = streaming_view(&state).expect("streaming");
        assert!(
            matches!(view.blocks.as_slice(), [Block::Text(text)] if !text.is_empty()),
            "{view:?}"
        );
    }

    // spec: UI-043
    #[test]
    fn notices_go_after_the_messages_that_preceded_them() {
        let mut state = fixture("text_reply");
        let frame = NoticeFrame::Unknown(json!({"type": "mystery"}));
        state.notices = vec![
            Notice {
                position: 1,
                frame: frame.clone(),
            },
            Notice {
                position: 2,
                frame: frame.clone(),
            },
            Notice { position: 9, frame },
        ];
        assert_eq!(
            kinds(&state),
            vec!["user", "notice", "assistant", "notice", "notice"]
        );
        let keys: Vec<String> = items(&state).into_iter().map(|item| item.key).collect();
        assert_eq!(keys, vec!["m0", "n0", "m1", "n1", "n2"]);
    }

    // spec: UI-041
    #[test]
    fn thinking_is_its_own_block_unless_it_is_narration() {
        let message: AgentMessage = serde_json::from_value(json!({
            "role": "assistant",
            "content": [
                {"type": "thinking", "thinking": "let me think"},
                {"type": "thinking", "thinking": "Here is the plan", "narration": true},
                {"type": "text", "text": "done"}
            ]
        }))
        .expect("message");
        let mut state = ThreadState::default();
        state.transcript.push(omfp_core::TranscriptEntry {
            message_id: None,
            message,
        });
        let blocks = message_view(&state, 0).blocks;
        assert_eq!(
            blocks,
            vec![
                Block::Thinking("let me think".to_owned()),
                Block::Text("Here is the plan".to_owned()),
                Block::Text("done".to_owned()),
            ]
        );
    }

    // spec: UI-043
    #[test]
    fn other_message_kinds_render_as_records_with_their_json() {
        let message: AgentMessage = serde_json::from_value(json!({
            "role": "bashExecution", "command": "ls", "output": "a\nb", "exitCode": 0
        }))
        .expect("message");
        let mut state = ThreadState::default();
        state.transcript.push(omfp_core::TranscriptEntry {
            message_id: None,
            message,
        });
        let view = message_view(&state, 0);
        assert_eq!((view.role, view.label.as_str()), (Role::Record, "bash"));
        let [Block::Record { summary, json }] = view.blocks.as_slice() else {
            panic!("{view:?}")
        };
        assert_eq!(summary, "ls");
        assert!(json.contains("\"exitCode\": 0"), "{json}");
    }
}
