//! Golden states (ARC-006, TST-012): for every recorded thread fixture, the state after each event as canonical JSON
//! in `fixtures/states/<OMP_VERSION>/<name>.jsonl`, one line per event. Other reducer implementations (the
//! TypeScript port in `ui/packages/core`) replay the same fixtures and must reproduce every line.
//!
//! `OMFP_UPDATE_STATES=1` (`just gen-states`) rewrites the files and removes state files without a fixture.

mod common;

use std::path::{Path, PathBuf};

use common::{fixture, fixture_dir, jsonl_names};
use omfp_core::{ThreadEvent, ThreadState};
use serde_json::Value;

const UPDATE_VAR: &str = "OMFP_UPDATE_STATES";

fn state_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/states")
        .join(omfp_core::OMP_VERSION)
}

/// `value` with every object's keys sorted, whatever map type `serde_json` was built with.
fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|(left, _), (right, _)| left.cmp(right));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonical(value)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        other => other,
    }
}

/// One compact canonical JSON line per event: the state after applying it.
fn render(events: &[ThreadEvent]) -> String {
    let mut state = ThreadState::default();
    let mut out = String::new();
    for event in events {
        state.apply(event);
        let value = serde_json::to_value(&state).expect("serializable state");
        out.push_str(&serde_json::to_string(&canonical(value)).expect("serializable value"));
        out.push('\n');
    }
    out
}

fn state_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.jsonl"))
}

fn update(dir: &Path, rendered: &[(String, String)]) {
    std::fs::create_dir_all(dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));
    for name in jsonl_names(dir) {
        if !rendered.iter().any(|(fixture, _)| *fixture == name) {
            let path = state_path(dir, &name);
            std::fs::remove_file(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        }
    }
    for (name, text) in rendered {
        let path = state_path(dir, name);
        std::fs::write(&path, text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    }
}

/// Describes the first line where `actual` and `expected` differ.
fn first_difference(actual: &str, expected: &str) -> String {
    let actual_lines: Vec<&str> = actual.lines().collect();
    let expected_lines: Vec<&str> = expected.lines().collect();
    let index = actual_lines
        .iter()
        .zip(&expected_lines)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| actual_lines.len().min(expected_lines.len()));
    format!(
        "{} lines on disk, {} rendered; first difference at event {index}:\n  on disk:  {}\n  rendered: {}",
        actual_lines.len(),
        expected_lines.len(),
        actual_lines.get(index).unwrap_or(&"<none>"),
        expected_lines.get(index).unwrap_or(&"<none>"),
    )
}

// spec: ARC-006, TST-012
#[test]
fn golden_states_equal_the_replayed_fixtures() {
    let rendered: Vec<(String, String)> = jsonl_names(&fixture_dir())
        .into_iter()
        .map(|name| {
            let text = render(&fixture(&name));
            (name, text)
        })
        .collect();
    assert_ne!(
        rendered,
        Vec::new(),
        "no fixtures in {}",
        fixture_dir().display()
    );
    let dir = state_dir();
    if std::env::var(UPDATE_VAR).is_ok_and(|value| value == "1") {
        update(&dir, &rendered);
    }

    // A missing directory has no stale files; the per-fixture check below then reports the missing states.
    let existing = if dir.exists() {
        jsonl_names(&dir)
    } else {
        Vec::new()
    };
    let stale: Vec<String> = existing
        .into_iter()
        .filter(|name| !rendered.iter().any(|(fixture, _)| fixture == name))
        .collect();
    assert_eq!(
        stale,
        Vec::<String>::new(),
        "state files without a fixture in {}: run `just gen-states`",
        dir.display()
    );
    for (name, expected) in &rendered {
        let path = state_path(&dir, name);
        let actual = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{}: {error}: run `just gen-states`", path.display()));
        assert!(
            actual == *expected,
            "{name}: golden states differ from the reducer: run `just gen-states` and review the diff\n{}",
            first_difference(&actual, expected)
        );
    }
}
