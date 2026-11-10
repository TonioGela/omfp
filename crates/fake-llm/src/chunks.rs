//! Encoding of a [`Reply`] as OpenAI chat-completion objects (streamed chunks or one complete message).

use serde_json::{Value, json};

use crate::script::Reply;

/// Characters per streamed text delta. Small, so even short replies arrive in several chunks.
const TEXT_CHUNK_CHARS: usize = 4;
/// Characters per streamed tool-call argument fragment.
const ARGUMENT_CHUNK_CHARS: usize = 8;

/// Splits `text` into pieces of at most `size` characters (never inside a UTF-8 character).
fn split_chars(text: &str, size: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(size)
        .map(|piece| piece.iter().collect())
        .collect()
}

/// Rough token count used for `usage`; omp only needs plausible numbers.
fn approx_tokens(text: &str) -> u64 {
    u64::try_from(text.len().div_ceil(4)).unwrap_or(u64::MAX)
}

fn chunk(id: &str, model: &str, delta: &Value, finish_reason: Option<&str>) -> Value {
    json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": 0,
        "model": model,
        "choices": [ { "index": 0, "delta": delta, "finish_reason": finish_reason } ],
    })
}

fn usage(request: &Value, reply: &Reply) -> Value {
    let prompt = approx_tokens(&request.to_string());
    let completion = approx_tokens(reply.text.as_deref().unwrap_or_default())
        + reply
            .tool_calls
            .iter()
            .map(|call| approx_tokens(&call.arguments.to_string()))
            .sum::<u64>();
    json!({ "prompt_tokens": prompt, "completion_tokens": completion, "total_tokens": prompt + completion })
}

/// The streamed chunks for `reply`: role, text deltas, tool-call fragments, finish reason, usage.
pub(crate) fn stream_chunks(id: &str, model: &str, request: &Value, reply: &Reply) -> Vec<Value> {
    let mut chunks = vec![chunk(
        id,
        model,
        &json!({ "role": "assistant", "content": "" }),
        None,
    )];

    if let Some(text) = &reply.text {
        for piece in split_chars(text, TEXT_CHUNK_CHARS) {
            chunks.push(chunk(id, model, &json!({ "content": piece }), None));
        }
    }

    for (index, call) in reply.tool_calls.iter().enumerate() {
        let opening = json!({ "tool_calls": [ {
            "index": index,
            "id": call.id,
            "type": "function",
            "function": { "name": call.name, "arguments": "" },
        } ] });
        chunks.push(chunk(id, model, &opening, None));
        for fragment in split_chars(&call.arguments.to_string(), ARGUMENT_CHUNK_CHARS) {
            let delta = json!({ "tool_calls": [ { "index": index, "function": { "arguments": fragment } } ] });
            chunks.push(chunk(id, model, &delta, None));
        }
    }

    chunks.push(chunk(id, model, &json!({}), Some(reply.finish_reason())));
    chunks.push(json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": 0,
        "model": model,
        "choices": [],
        "usage": usage(request, reply),
    }));
    chunks
}

/// Server-sent events body: one `data:` event per chunk, then `data: [DONE]`.
pub(crate) fn sse_body(chunks: &[Value]) -> String {
    let mut body = String::new();
    for chunk in chunks {
        body.push_str("data: ");
        body.push_str(&chunk.to_string());
        body.push_str("\n\n");
    }
    body.push_str("data: [DONE]\n\n");
    body
}

/// Non-streaming answer: the whole reply as one `chat.completion`.
pub(crate) fn completion(id: &str, model: &str, request: &Value, reply: &Reply) -> Value {
    let tool_calls: Vec<Value> = reply
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "id": call.id,
                "type": "function",
                "function": { "name": call.name, "arguments": call.arguments.to_string() },
            })
        })
        .collect();
    let mut message = json!({ "role": "assistant", "content": reply.text });
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }
    json!({
        "id": id,
        "object": "chat.completion",
        "created": 0,
        "model": model,
        "choices": [ { "index": 0, "message": message, "finish_reason": reply.finish_reason() } ],
        "usage": usage(request, reply),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::ToolCall;

    #[test]
    fn split_chars_respects_multibyte_characters() {
        assert_eq!(split_chars("héllo wörld", 4), ["héll", "o wö", "rld"]);
        assert_eq!(split_chars("", 4), Vec::<String>::new());
    }

    #[test]
    fn sse_body_frames_each_chunk_and_terminates_with_done() {
        let body = sse_body(&[json!({ "a": 1 }), json!({ "b": 2 })]);
        assert_eq!(
            body,
            "data: {\"a\":1}\n\ndata: {\"b\":2}\n\ndata: [DONE]\n\n"
        );
    }

    #[test]
    fn text_and_tool_calls_share_one_stream_with_tool_call_finish() {
        let reply = Reply {
            text: Some("ok".into()),
            tool_calls: vec![ToolCall::new("c1", "read", json!({ "path": "a" }))],
        };
        let chunks = stream_chunks("id", "m", &json!({}), &reply);
        let finish: Vec<&str> = chunks
            .iter()
            .filter_map(|c| c["choices"][0]["finish_reason"].as_str())
            .collect();
        assert_eq!(finish, ["tool_calls"]);
        assert!(
            chunks
                .iter()
                .any(|c| c["choices"][0]["delta"]["content"] == "ok")
        );
    }

    #[test]
    fn completion_carries_tool_call_arguments_as_a_json_string() {
        let reply = Reply::tool_calls(vec![ToolCall::new("c1", "read", json!({ "path": "a" }))]);
        let body = completion("id", "m", &json!({}), &reply);
        assert_eq!(
            body["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
            "{\"path\":\"a\"}"
        );
        assert_eq!(body["choices"][0]["finish_reason"], "tool_calls");
    }
}
