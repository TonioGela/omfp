use serde::{Deserialize, Serialize};
use serde_json::Value;

/// An ordered list of replies the fake model gives, one per chat request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Script {
    model: String,
    steps: Vec<Step>,
}

impl Script {
    #[must_use]
    pub fn new(model: impl Into<String>, steps: Vec<Step>) -> Self {
        Self {
            model: model.into(),
            steps,
        }
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    pub(crate) fn into_parts(self) -> (String, Vec<Step>) {
        (self.model, self.steps)
    }
}

/// One expected request and the reply to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Matcher>,
    pub reply: Reply,
}

impl Step {
    /// Replies to the next request, whatever it contains.
    #[must_use]
    pub fn reply(reply: Reply) -> Self {
        Self { when: None, reply }
    }

    /// Replies only if the request satisfies `matcher`; otherwise the request fails the run.
    #[must_use]
    pub fn when(matcher: Matcher, reply: Reply) -> Self {
        Self {
            when: Some(matcher),
            reply,
        }
    }
}

/// Condition a request must meet for its step to answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Matcher {
    /// The text of the last `user` message contains this substring.
    LastUserContains(String),
}

impl Matcher {
    pub(crate) fn matches(&self, request: &Value) -> bool {
        match self {
            Self::LastUserContains(needle) => {
                last_user_text(request).is_some_and(|text| text.contains(needle.as_str()))
            }
        }
    }
}

/// Text of the last `user` message, whether `content` is a string or a list of parts.
pub(crate) fn last_user_text(request: &Value) -> Option<String> {
    let message = request["messages"]
        .as_array()?
        .iter()
        .rev()
        .find(|message| message["role"] == "user")?;
    match &message["content"] {
        Value::String(text) => Some(text.clone()),
        Value::Array(parts) => Some(
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect(),
        ),
        _ => None,
    }
}

/// What the assistant answers: optional text, then optional tool calls.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl Reply {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            tool_calls: Vec::new(),
        }
    }

    #[must_use]
    pub fn tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            text: None,
            tool_calls,
        }
    }

    pub(crate) fn finish_reason(&self) -> &'static str {
        if self.tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        }
    }
}

/// A function call the assistant makes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

impl ToolCall {
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>, arguments: Value) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn last_user_text_reads_string_and_part_list_content() {
        let plain = json!({ "messages": [
            { "role": "user", "content": "first" },
            { "role": "assistant", "content": "x" },
            { "role": "user", "content": "second" }
        ]});
        assert_eq!(last_user_text(&plain).as_deref(), Some("second"));

        let parts = json!({ "messages": [
            { "role": "user", "content": [ { "type": "text", "text": "a" }, { "type": "image_url" }, { "type": "text", "text": "b" } ] }
        ]});
        assert_eq!(last_user_text(&parts).as_deref(), Some("ab"));

        assert_eq!(last_user_text(&json!({ "messages": [] })), None);
    }

    #[test]
    fn finish_reason_depends_on_tool_calls() {
        assert_eq!(Reply::text("x").finish_reason(), "stop");
        assert_eq!(
            Reply::tool_calls(vec![ToolCall::new("c", "t", json!({}))]).finish_reason(),
            "tool_calls"
        );
    }
}
