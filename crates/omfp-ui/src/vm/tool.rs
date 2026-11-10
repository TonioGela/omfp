//! The fallback tool card (UI-050): name, one-line argument summary, status, duration, and expandable pretty JSON
//! arguments and text result. Failed tools are expanded by default (UI-052).

use omfp_core::wire::{ToolCall, ToolResultMessage, UserContent};
use omfp_core::{ToolRun, ToolStatus};
use serde_json::{Map, Value};

use super::{one_line, pretty_json};

/// Key of a streaming tool-call block holding its argument JSON received so far.
const PARTIAL_ARGS: &str = "partialArgs";

/// Longest argument summary.
const SUMMARY_CHARS: usize = 96;

/// Progress shown on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardStatus {
    /// The model is still writing the arguments.
    Composing,
    Pending,
    Running,
    Succeeded,
    Failed,
}

impl CardStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Composing => "writing arguments",
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "done",
            Self::Failed => "failed",
        }
    }

    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Composing => "composing",
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

/// Everything the fallback card shows.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCard {
    pub tool_call_id: String,
    pub name: String,
    pub summary: String,
    pub status: CardStatus,
    /// Milliseconds from the end of the calling message to the result, when both carry timestamps.
    pub duration_ms: Option<f64>,
    /// Pretty JSON, or the partial argument text while composing.
    pub args: String,
    /// Text parts of the result (or of the latest partial result), else its pretty JSON.
    pub result: Option<String>,
}

impl ToolCard {
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.status == CardStatus::Failed
    }

    /// Cards start collapsed, failed ones expanded (UI-052).
    #[must_use]
    pub fn expanded_by_default(&self) -> bool {
        self.is_error()
    }
}

/// The card of one tool call. `call` is the assistant's tool-call block (absent when only the run is known), `run` the
/// execution tracked by the reducer, `result` the transcript's tool result message, `called_at` the calling message's
/// completion timestamp.
pub fn tool_card(
    tool_call_id: &str,
    call: Option<&ToolCall>,
    run: Option<&ToolRun>,
    result: Option<&ToolResultMessage>,
    called_at: Option<f64>,
) -> ToolCard {
    let name = call
        .and_then(|call| call.name.clone())
        .or_else(|| run.map(|run| run.tool_name.clone()))
        .or_else(|| result.and_then(|result| result.tool_name.clone()))
        .unwrap_or_else(|| "tool".to_owned());
    let partial = call
        .and_then(|call| call.extra.get(PARTIAL_ARGS))
        .and_then(Value::as_str);
    let arguments: Option<Value> = call
        .and_then(|call| call.arguments.clone().map(Value::Object))
        .or_else(|| run.and_then(|run| run.args.clone()));
    let intent = call
        .and_then(|call| call.intent.as_deref())
        .or_else(|| run.and_then(|run| run.intent.as_deref()));
    let status = status(partial.is_some() && arguments.is_none(), run, result);
    let summary = match (intent, &arguments, partial) {
        (Some(intent), _, _) => one_line(intent, SUMMARY_CHARS),
        (None, Some(Value::Object(arguments)), _) => summarize(arguments),
        (None, _, Some(partial)) => one_line(partial, SUMMARY_CHARS),
        _ => String::new(),
    };
    let args = match (&arguments, partial) {
        (Some(arguments), _) => pretty_json(arguments),
        (None, Some(partial)) => partial.to_owned(),
        (None, None) => "{}".to_owned(),
    };
    let result_text = run
        .and_then(|run| run.result.as_ref().or(run.partial_result.as_ref()))
        .map(result_text)
        .or_else(|| {
            result
                .and_then(|result| result.content.as_deref())
                .map(user_content_text)
        });
    let finished_at = result.and_then(|result| result.timestamp);
    let duration_ms = match (called_at, finished_at) {
        (Some(start), Some(end)) if start > 0.0 && end >= start => Some(end - start),
        _ => None,
    };
    ToolCard {
        tool_call_id: tool_call_id.to_owned(),
        name,
        summary,
        status,
        duration_ms,
        args,
        result: result_text,
    }
}

fn status(
    composing: bool,
    run: Option<&ToolRun>,
    result: Option<&ToolResultMessage>,
) -> CardStatus {
    let failed = run.is_some_and(|run| run.status == ToolStatus::Failed)
        || result.is_some_and(|result| result.is_error == Some(true));
    if failed {
        return CardStatus::Failed;
    }
    match (run.map(|run| run.status), result) {
        (Some(ToolStatus::Succeeded), _) | (None, Some(_)) => CardStatus::Succeeded,
        (Some(ToolStatus::Running), _) => CardStatus::Running,
        (None, None) if composing => CardStatus::Composing,
        _ => CardStatus::Pending,
    }
}

/// `key: value` of the scalar arguments, by key, on one line.
fn summarize(arguments: &Map<String, Value>) -> String {
    let parts: Vec<String> = arguments
        .iter()
        .filter_map(|(key, value)| {
            let value = match value {
                Value::String(text) => text.clone(),
                Value::Number(number) => number.to_string(),
                Value::Bool(flag) => flag.to_string(),
                _ => return None,
            };
            Some(format!("{key}: {value}"))
        })
        .collect();
    one_line(&parts.join(", "), SUMMARY_CHARS)
}

/// The text parts of a tool result value (`{content: [{type: "text", text}], …}`), else its pretty JSON.
pub fn result_text(value: &Value) -> String {
    let texts: Option<Vec<&str>> = value
        .get("content")
        .and_then(Value::as_array)
        .map(|content| {
            content
                .iter()
                .filter(|part| part.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect()
        });
    match (texts, value) {
        (Some(texts), _) if !texts.is_empty() => texts.join("\n"),
        (_, Value::String(text)) => text.clone(),
        _ => pretty_json(value),
    }
}

fn user_content_text(content: &[UserContent]) -> String {
    content
        .iter()
        .map(|part| match part {
            UserContent::Text(text) => text.text.clone().unwrap_or_default(),
            UserContent::Image(_) => "[image]".to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `1.2 s`, `350 ms`, `2 min 5 s`.
#[must_use]
pub fn format_duration(ms: f64) -> String {
    if ms < 1000.0 {
        return format!("{ms:.0} ms");
    }
    let seconds = ms / 1000.0;
    if seconds < 60.0 {
        return format!("{seconds:.1} s");
    }
    let minutes = (seconds / 60.0).floor();
    format!("{minutes:.0} min {:.0} s", seconds - minutes * 60.0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::test_support::{fixture, fixture_until};
    use crate::vm::transcript::{Block, ItemKind, items, message_view};

    fn cards(state: &omfp_core::ThreadState) -> Vec<ToolCard> {
        let mut cards = Vec::new();
        for item in items(state) {
            if let ItemKind::Message(index) = item.kind {
                for block in message_view(state, index).blocks {
                    if let Block::Tool(card) = block {
                        cards.push(card);
                    }
                }
            }
        }
        cards
    }

    // spec: UI-050, UI-011
    #[test]
    fn a_finished_bash_call_renders_through_the_fallback_card() {
        let state = fixture("bash_tool_call");
        let cards = cards(&state);
        assert_eq!(cards.len(), 1);
        let card = &cards[0];
        assert_eq!(
            (card.name.as_str(), card.status),
            ("bash", CardStatus::Succeeded)
        );
        assert!(
            card.summary.contains("echo tool-output-123"),
            "{}",
            card.summary
        );
        assert!(card.args.contains("\"command\""), "{}", card.args);
        assert_eq!(card.result.as_deref(), Some("tool-output-123\n"));
        assert!(!card.expanded_by_default());
    }

    // spec: UI-050
    #[test]
    fn a_running_call_shows_its_partial_result() {
        let state = fixture_until("bash_tool_call", |event| {
            event.pointer("/frame/type").and_then(Value::as_str) == Some("tool_execution_end")
        });
        let run = state.tools.first().expect("a run");
        let card = tool_card(&run.tool_call_id, None, Some(run), None, None);
        assert_eq!(card.status, CardStatus::Running);
    }

    // spec: UI-050
    #[test]
    fn while_streaming_the_card_shows_the_partial_argument_text() {
        let mut call = ToolCall {
            name: Some("bash".to_owned()),
            ..ToolCall::default()
        };
        call.extra
            .insert(PARTIAL_ARGS.to_owned(), json!("{\"command\": \"ec"));
        let card = tool_card("c1", Some(&call), None, None, None);
        assert_eq!(card.status, CardStatus::Composing);
        assert_eq!(card.args, "{\"command\": \"ec");
        assert_eq!(card.summary, "{\"command\": \"ec");
    }

    // spec: UI-052, UI-011
    #[test]
    fn failed_tools_are_expanded_by_default_and_take_the_error_text() {
        let run = ToolRun {
            tool_call_id: "c1".to_owned(),
            tool_name: "read".to_owned(),
            args: Some(json!({"path": "missing.txt", "limit": 5, "nested": {"x": 1}})),
            intent: None,
            status: ToolStatus::Failed,
            partial_result: None,
            preview: None,
            result: Some(json!({"content": [{"type": "text", "text": "ENOENT"}]})),
        };
        let card = tool_card("c1", None, Some(&run), None, None);
        assert!(card.is_error());
        assert!(card.expanded_by_default());
        assert_eq!(card.summary, "limit: 5, path: missing.txt");
        assert_eq!(card.result.as_deref(), Some("ENOENT"));
    }

    // spec: UI-050
    #[test]
    fn duration_runs_from_the_calling_message_to_the_result() {
        let result = ToolResultMessage {
            timestamp: Some(3500.0),
            ..ToolResultMessage::default()
        };
        let card = tool_card("c1", None, None, Some(&result), Some(1000.0));
        assert_eq!(
            (card.status, card.duration_ms),
            (CardStatus::Succeeded, Some(2500.0))
        );
        assert_eq!(format_duration(2500.0), "2.5 s");
        assert_eq!(format_duration(125_000.0), "2 min 5 s");
        assert_eq!(format_duration(40.0), "40 ms");
    }
}
