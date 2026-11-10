//! Full-text search over the conversations of threads (DMN-076): the user and assistant text in each thread's omp
//! session files (`<session_dir>/*.jsonl`), read directly, without starting omp.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

/// At most this many results.
pub const MAX_RESULTS: usize = 200;
/// Shorter queries (in characters, after trimming) find nothing.
pub const MIN_QUERY_CHARS: usize = 2;
/// Characters of context kept before and after the match in a snippet.
const CONTEXT_BEFORE: usize = 30;
const CONTEXT_AFTER: usize = 60;

/// A thread whose conversation contains the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub thread_id: String,
    /// The text around the first match, whitespace collapsed, `…` where cut.
    pub snippet: String,
}

/// The threads (`(id, session_dir)`) whose conversation contains `query`, case-insensitive: most recently written
/// session first, at most [`MAX_RESULTS`]. Unreadable files and lines that are not session entries are skipped.
#[must_use]
pub fn search<'a>(threads: impl IntoIterator<Item = (&'a str, &'a Path)>, query: &str) -> Vec<Hit> {
    let needle: Vec<char> = query.trim().chars().flat_map(char::to_lowercase).collect();
    if query.trim().chars().count() < MIN_QUERY_CHARS {
        return Vec::new();
    }
    let mut hits: Vec<(SystemTime, Hit)> = threads
        .into_iter()
        .filter_map(|(id, dir)| {
            let files = session_files(dir);
            let modified = files
                .iter()
                .filter_map(|file| file.metadata().and_then(|meta| meta.modified()).ok())
                .max()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let snippet = files.iter().find_map(|file| first_match(file, &needle))?;
            Some((
                modified,
                Hit {
                    thread_id: id.to_owned(),
                    snippet,
                },
            ))
        })
        .collect();
    hits.sort_by(|(a, hit_a), (b, hit_b)| {
        b.cmp(a).then_with(|| hit_a.thread_id.cmp(&hit_b.thread_id))
    });
    hits.truncate(MAX_RESULTS);
    hits.into_iter().map(|(_, hit)| hit).collect()
}

/// The session files of a thread, oldest name first (names start with a timestamp).
fn session_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "jsonl") && path.is_file())
        .collect();
    files.sort();
    files
}

/// The snippet of the first user or assistant text in `file` containing `needle` (lowercase characters).
fn first_match(file: &Path, needle: &[char]) -> Option<String> {
    let reader = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    reader
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .find_map(|entry| {
            message_texts(&entry)
                .into_iter()
                .find_map(|text| snippet(text, needle))
        })
}

/// The texts of a `message` entry with role `user` or `assistant`: its string content, or its `text` blocks.
fn message_texts(entry: &Value) -> Vec<&str> {
    if entry["type"] != "message" {
        return Vec::new();
    }
    let message = &entry["message"];
    if !matches!(message["role"].as_str(), Some("user" | "assistant")) {
        return Vec::new();
    }
    match &message["content"] {
        Value::String(text) => vec![text.as_str()],
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .collect(),
        _ => Vec::new(),
    }
}

/// The context of the first case-insensitive occurrence of `needle` in `text`, if any.
fn snippet(text: &str, needle: &[char]) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    // Each lowercase character with the index of the original character it comes from.
    let lowered: Vec<(usize, char)> = chars
        .iter()
        .enumerate()
        .flat_map(|(index, c)| c.to_lowercase().map(move |lower| (index, lower)))
        .collect();
    let start = lowered
        .windows(needle.len())
        .position(|window| window.iter().map(|(_, c)| c).eq(needle.iter()))?;
    let first = lowered[start].0;
    let last = lowered[start + needle.len() - 1].0;
    let from = first.saturating_sub(CONTEXT_BEFORE);
    let to = (last + 1 + CONTEXT_AFTER).min(chars.len());
    let body: String = chars[from..to].iter().collect();
    let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let lead = if from > 0 { "…" } else { "" };
    let tail = if to < chars.len() { "…" } else { "" };
    Some(format!("{lead}{body}{tail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(query: &str) -> Vec<char> {
        query.chars().flat_map(char::to_lowercase).collect()
    }

    // spec: DMN-076
    #[test]
    fn snippets_match_case_insensitively_and_cut_with_an_ellipsis() {
        assert_eq!(
            snippet("Why is the\ncheckout test FLAKY?", &lower("flaky")),
            Some("Why is the checkout test FLAKY?".to_owned())
        );
        let long = format!("{} needle {}", "a".repeat(100), "b".repeat(100));
        let found = snippet(&long, &lower("NEEDLE")).unwrap();
        assert!(found.starts_with('…') && found.ends_with('…'), "{found}");
        assert!(found.contains("needle"), "{found}");
        assert_eq!(
            snippet("İstanbul", &lower("i̇st")),
            Some("İstanbul".to_owned())
        );
        assert_eq!(snippet("nothing here", &lower("absent")), None);
    }

    // spec: DMN-076
    #[test]
    fn only_user_and_assistant_text_counts() {
        let entry = |role: &str, content: Value| serde_json::json!({ "type": "message", "message": { "role": role, "content": content } });
        assert_eq!(message_texts(&entry("user", "hello".into())), ["hello"]);
        assert_eq!(
            message_texts(&entry(
                "assistant",
                serde_json::json!([
                    { "type": "thinking", "thinking": "hidden" },
                    { "type": "text", "text": "shown" },
                    { "type": "toolCall", "name": "bash" }
                ])
            )),
            ["shown"]
        );
        assert_eq!(
            message_texts(&entry("toolResult", "output".into())),
            Vec::<&str>::new()
        );
        assert_eq!(
            message_texts(&serde_json::json!({ "type": "custom", "data": "x" })),
            Vec::<&str>::new()
        );
    }
}
