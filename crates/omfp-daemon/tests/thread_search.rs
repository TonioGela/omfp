//! L2: full-text search over threads' omp session files (`threads.search`, DMN-076), written in omp's session format
//! (`docs/session.md` of the pinned omp: a `session` header, then one entry per line).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use omfp_daemon::search::{Hit, MAX_RESULTS, search};
use serde_json::{Value, json};
use tempfile::TempDir;

fn header() -> Value {
    json!({ "type": "session", "version": 3, "id": "s1", "timestamp": "2026-11-10T10:00:00.000Z", "cwd": "/ws" })
}

fn message(role: &str, content: Value) -> Value {
    let mut entry = json!({
        "type": "message", "id": "e1", "parentId": null, "timestamp": "2026-11-10T10:00:01.000Z",
        "message": { "role": role, "timestamp": 1_794_268_801_000_i64 }
    });
    entry["message"]["content"] = content;
    entry
}

/// Writes `entries` (then a line that is no JSON) as the session file `name` of `dir`, last modified `modified`
/// seconds after the epoch.
fn session(dir: &Path, name: &str, entries: &[Value], modified: u64) {
    std::fs::create_dir_all(dir).unwrap();
    let mut text: String = entries
        .iter()
        .map(|entry| entry.to_string() + "\n")
        .collect();
    text.push_str("not json\n");
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(modified))
        .unwrap();
}

struct Threads {
    _dir: TempDir,
    dirs: Vec<(String, PathBuf)>,
}

impl Threads {
    fn search(&self, query: &str) -> Vec<Hit> {
        search(
            self.dirs
                .iter()
                .map(|(id, dir)| (id.as_str(), dir.as_path())),
            query,
        )
    }

    fn ids(&self, query: &str) -> Vec<String> {
        self.search(query)
            .into_iter()
            .map(|hit| hit.thread_id)
            .collect()
    }
}

/// `t-old` (user text as a string), `t-new` (assistant text blocks, written later), `t-tool` (the word only in a tool
/// result and a custom entry), `t-none` (no session directory).
fn threads() -> Threads {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_owned();
    let path = |id: &str| root.join(id).join("sessions");
    session(
        &path("t-old"),
        "2026-11-10T10-00-00_s1.jsonl",
        &[
            header(),
            message("user", json!("Why is the checkout test Flaky on CI?")),
        ],
        1_000,
    );
    session(
        &path("t-new"),
        "2026-11-10T11-00-00_s2.jsonl",
        &[
            header(),
            message(
                "user",
                json!([{ "type": "text", "text": "Look at the login form." }]),
            ),
            message(
                "assistant",
                json!([
                    { "type": "thinking", "thinking": "flaky? no" },
                    { "type": "text", "text": "The login form test is flaky because of a timer." }
                ]),
            ),
        ],
        2_000,
    );
    session(
        &path("t-tool"),
        "2026-11-10T12-00-00_s3.jsonl",
        &[
            header(),
            message(
                "toolResult",
                json!([{ "type": "text", "text": "flaky output" }]),
            ),
            json!({ "type": "custom", "customType": "note", "data": { "text": "flaky" } }),
        ],
        3_000,
    );
    let dirs = ["t-old", "t-new", "t-tool", "t-none"]
        .into_iter()
        .map(|id| (id.to_owned(), path(id)))
        .collect();
    Threads { _dir: dir, dirs }
}

// spec: DMN-076
#[test]
fn user_and_assistant_text_matches_case_insensitively_most_recent_first() {
    let threads = threads();
    assert_eq!(threads.ids("FLAKY"), ["t-new", "t-old"]);
    assert_eq!(threads.ids("login form"), ["t-new"]);
    assert_eq!(threads.ids("nowhere to be found"), Vec::<String>::new());
}

// spec: DMN-076
#[test]
fn the_snippet_is_the_text_around_the_first_match() {
    let threads = threads();
    let hits = threads.search("flaky");
    assert_eq!(
        hits[0].snippet,
        "The login form test is flaky because of a timer."
    );
    assert_eq!(hits[1].snippet, "Why is the checkout test Flaky on CI?");
}

// spec: DMN-076
#[test]
fn queries_shorter_than_two_characters_find_nothing() {
    let threads = threads();
    assert_eq!(threads.ids("a"), Vec::<String>::new());
    assert_eq!(threads.ids("  f  "), Vec::<String>::new());
    assert_eq!(threads.ids("ci"), ["t-old"]);
}

// spec: DMN-076
#[test]
fn results_are_capped() {
    let dir = TempDir::new().unwrap();
    let dirs: Vec<(String, PathBuf)> = (0..MAX_RESULTS + 3)
        .map(|index| {
            let path = dir.path().join(format!("t{index}"));
            session(&path, "s.jsonl", &[message("user", json!("needle"))], 1);
            (format!("t{index}"), path)
        })
        .collect();
    let threads = Threads { _dir: dir, dirs };
    assert_eq!(threads.search("needle").len(), MAX_RESULTS);
}
