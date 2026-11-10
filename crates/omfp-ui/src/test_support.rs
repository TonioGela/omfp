//! Test inputs: thread states replayed from the recorded fixtures (TST-012) and hand-built DTOs.

use std::path::PathBuf;

use omfp_core::{OMP_VERSION, ProcessState, ThreadEvent, ThreadState};
use omfp_protocol::{CreatedBy, Provenance, Repository, ThreadSummary, Workspace, WorkspaceKind};

/// Events of `fixtures/threads/<omp version>/<name>.jsonl`.
pub fn fixture_events(name: &str) -> Vec<ThreadEvent> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/threads")
        .join(OMP_VERSION)
        .join(format!("{name}.jsonl"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line).unwrap_or_else(|error| panic!("{name}: {error}: {line}"))
        })
        .collect()
}

/// The state after replaying the whole fixture.
pub fn fixture(name: &str) -> ThreadState {
    omfp_core::replay(&fixture_events(name))
}

/// The state after replaying the fixture's events up to (excluding) the first one `stop` accepts.
pub fn fixture_until(name: &str, stop: impl Fn(&serde_json::Value) -> bool) -> ThreadState {
    let events = fixture_events(name);
    let end = events
        .iter()
        .position(|event| stop(&serde_json::to_value(event).expect("event serializes")))
        .unwrap_or(events.len());
    omfp_core::replay(&events[..end])
}

/// The state after replaying the fixture's first `count` events.
pub fn fixture_prefix(name: &str, count: usize) -> ThreadState {
    let events = fixture_events(name);
    omfp_core::replay(&events[..count.min(events.len())])
}

pub fn provenance() -> Provenance {
    Provenance {
        created_by: CreatedBy::User,
        client: None,
    }
}

pub fn repository(id: &str, name: &str) -> Repository {
    Repository {
        id: id.to_owned(),
        name: name.to_owned(),
        path: format!("/src/{name}"),
        default_base_ref: "main".to_owned(),
        remote_url: None,
        created_at: 1,
    }
}

/// A managed worktree of `repository` on `branch`.
pub fn workspace(id: &str, repository: Option<&str>, branch: Option<&str>) -> Workspace {
    Workspace {
        id: id.to_owned(),
        path: format!("/work/{id}"),
        kind: if repository.is_some() {
            WorkspaceKind::ManagedWorktree
        } else {
            WorkspaceKind::Adopted
        },
        repository_id: repository.map(str::to_owned),
        branch: branch.map(str::to_owned),
        base_ref: None,
        pr_url: None,
        pr_number: None,
        provenance: provenance(),
        created_at: 1,
    }
}

/// An idle running thread on `workspace_ids`.
pub fn thread(id: &str, title: &str, workspace_ids: &[&str]) -> ThreadSummary {
    ThreadSummary {
        id: id.to_owned(),
        title: title.to_owned(),
        tags: Vec::new(),
        workspace_ids: workspace_ids.iter().map(|id| (*id).to_owned()).collect(),
        process: ProcessState::Running,
        settled: true,
        needs_attention: false,
        provenance: provenance(),
        created_at: 1,
        updated_at: 1,
        archived_at: None,
    }
}
