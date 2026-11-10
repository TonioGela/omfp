//! Rebuilding a streaming assistant message from `messageUpdates: "delta"` events (omp `docs/rpc.md`, "Event Stream
//! Schema"): `*_start` carries the block shell with its streamed content emptied, deltas append to the block at
//! `contentIndex`, and `*_end` is authoritative for that block.

use serde::Deserialize;
use serde_json::Value;

use crate::state::StreamingMessage;
use crate::wire::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, LitAssistant, TextContent,
    ThinkingContent, ToolCall,
};

/// Key of a tool-call block holding its argument JSON received so far (omp's own name for it).
const PARTIAL_ARGS: &str = "partialArgs";

/// The streamed string of a block kind.
#[derive(Debug, Clone, Copy)]
enum Streamed {
    Text,
    Thinking,
    ToolArgs,
}

impl Streamed {
    fn empty_block(self) -> AssistantContent {
        match self {
            Self::Text => AssistantContent::Text(TextContent::default()),
            Self::Thinking => AssistantContent::Thinking(ThinkingContent::default()),
            Self::ToolArgs => AssistantContent::ToolCall(ToolCall::default()),
        }
    }

    /// The string of `block` that deltas of this kind extend, if `block` is of this kind.
    fn of(self, block: &mut AssistantContent) -> Option<&mut String> {
        match (self, block) {
            (Self::Text, AssistantContent::Text(text)) => Some(text.text.get_or_insert_default()),
            (Self::Thinking, AssistantContent::Thinking(thinking)) => {
                Some(thinking.thinking.get_or_insert_default())
            }
            (Self::ToolArgs, AssistantContent::ToolCall(call)) => {
                let slot = call
                    .extra
                    .entry(PARTIAL_ARGS)
                    .or_insert_with(|| Value::String(String::new()));
                if !slot.is_string() {
                    *slot = Value::String(String::new());
                }
                match slot {
                    Value::String(partial) => Some(partial),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

impl StreamingMessage {
    /// An empty assistant message with id `message_id`.
    pub(crate) fn new(message_id: Option<String>) -> Self {
        Self {
            message_id,
            message: AssistantMessage {
                role: Some(LitAssistant),
                content: Some(Vec::new()),
                ..AssistantMessage::default()
            },
        }
    }

    /// Applies one delta-mode event. `done` and `error` carry nothing to rebuild: `message_end` follows.
    pub(crate) fn apply(&mut self, event: &AssistantMessageEvent) {
        match event {
            AssistantMessageEvent::Start(start) => {
                if let Some(message) = start
                    .extra
                    .get("message")
                    .and_then(|value| AssistantMessage::deserialize(value).ok())
                {
                    self.message = message;
                }
            }
            AssistantMessageEvent::TextStart(start) => {
                self.begin(
                    start.content_index,
                    start.extra.get("block"),
                    Streamed::Text,
                );
            }
            AssistantMessageEvent::ThinkingStart(start) => {
                self.begin(
                    start.content_index,
                    start.extra.get("block"),
                    Streamed::Thinking,
                );
            }
            AssistantMessageEvent::ToolcallStart(start) => {
                self.begin(
                    start.content_index,
                    start.extra.get("block"),
                    Streamed::ToolArgs,
                );
            }
            AssistantMessageEvent::TextDelta(delta) => {
                self.append(delta.content_index, delta.delta.as_deref(), Streamed::Text);
            }
            AssistantMessageEvent::ThinkingDelta(delta) => {
                self.append(
                    delta.content_index,
                    delta.delta.as_deref(),
                    Streamed::Thinking,
                );
            }
            AssistantMessageEvent::ToolcallDelta(delta) => {
                self.append(
                    delta.content_index,
                    delta.delta.as_deref(),
                    Streamed::ToolArgs,
                );
            }
            AssistantMessageEvent::TextEnd(end) => {
                self.finish(
                    end.content_index,
                    end.extra.get("block"),
                    end.content.as_deref(),
                    Streamed::Text,
                );
            }
            AssistantMessageEvent::ThinkingEnd(end) => {
                self.finish(
                    end.content_index,
                    end.extra.get("block"),
                    end.content.as_deref(),
                    Streamed::Thinking,
                );
            }
            AssistantMessageEvent::ImageEnd(end) => {
                if let (Some(index), Some(image)) = (slot(end.content_index), &end.content) {
                    self.put(index, AssistantContent::Image(image.clone()));
                }
            }
            AssistantMessageEvent::ToolcallEnd(end) => {
                if let (Some(index), Some(call)) = (slot(end.content_index), &end.tool_call) {
                    self.put(index, AssistantContent::ToolCall(call.clone()));
                }
            }
            AssistantMessageEvent::Done(_) | AssistantMessageEvent::Error(_) => {}
        }
    }

    /// `*_start`: the carried shell (or an empty block of `kind`) with its streamed string emptied.
    fn begin(&mut self, index: Option<i64>, shell: Option<&Value>, kind: Streamed) {
        let Some(index) = slot(index) else { return };
        let mut block = decode_block(shell).unwrap_or_else(|| kind.empty_block());
        if let Some(streamed) = kind.of(&mut block) {
            streamed.clear();
        }
        self.put(index, block);
    }

    /// `*_delta`: appends to the block at `index` when it is of `kind`.
    fn append(&mut self, index: Option<i64>, delta: Option<&str>, kind: Streamed) {
        let Some(delta) = delta else { return };
        let block = slot(index).and_then(|index| self.message.content.as_mut()?.get_mut(index));
        if let Some(streamed) = block.and_then(|block| kind.of(block)) {
            streamed.push_str(delta);
        }
    }

    /// `text_end` / `thinking_end`: the carried shell (else the rebuilt block) with `content` as its text.
    fn finish(
        &mut self,
        index: Option<i64>,
        shell: Option<&Value>,
        content: Option<&str>,
        kind: Streamed,
    ) {
        let Some(index) = slot(index) else { return };
        let mut block = decode_block(shell)
            .or_else(|| self.message.content.as_ref()?.get(index).cloned())
            .unwrap_or_else(|| kind.empty_block());
        if let (Some(content), Some(streamed)) = (content, kind.of(&mut block)) {
            content.clone_into(streamed);
        }
        self.put(index, block);
    }

    /// Sets the block at `index`, appending when the index is past the end.
    fn put(&mut self, index: usize, block: AssistantContent) {
        let content = self.message.content.get_or_insert_default();
        match content.get_mut(index) {
            Some(existing) => *existing = block,
            None => content.push(block),
        }
    }
}

fn slot(index: Option<i64>) -> Option<usize> {
    usize::try_from(index?).ok()
}

fn decode_block(shell: Option<&Value>) -> Option<AssistantContent> {
    AssistantContent::deserialize(shell?).ok()
}
