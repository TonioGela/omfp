//! Test helpers shared by the L0 and L1 test binaries: fixtures and DTO builders, a scripted in-memory transport and
//! a manual timer.

#![allow(dead_code)] // Each test binary uses a different subset.

pub mod fake;

use std::path::Path;

use omfp_core::{ProcessState, ThreadEvent};
use omfp_protocol::{
    CreatedBy, EventFrame, IndexEvent, Provenance, Repository, ThreadSummary, Topic, TopicEvent,
};

/// The `ThreadEvent`s of a recorded fixture (TST-012).
pub fn fixture(name: &str) -> Vec<ThreadEvent> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/threads")
        .join(omfp_core::OMP_VERSION)
        .join(format!("{name}.jsonl"));
    std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

pub const FIXTURES: [&str; 6] = [
    "text_reply",
    "bash_tool_call",
    "approval_granted",
    "ask_answered",
    "follow_up_then_abort",
    "restart_with_history",
];

pub fn thread_topic(id: &str) -> Topic {
    Topic::Thread(id.to_owned())
}

pub fn thread_frame(id: &str, epoch: &str, seq: u64, event: ThreadEvent) -> EventFrame {
    EventFrame {
        topic: thread_topic(id),
        epoch: epoch.to_owned(),
        seq,
        event: TopicEvent::Thread(event),
    }
}

pub fn index_frame(epoch: &str, seq: u64, event: IndexEvent) -> EventFrame {
    EventFrame {
        topic: Topic::Index,
        epoch: epoch.to_owned(),
        seq,
        event: TopicEvent::Index(event),
    }
}

fn provenance() -> Provenance {
    Provenance {
        created_by: CreatedBy::User,
        client: Some("test".into()),
    }
}

pub fn summary(id: &str, title: &str) -> ThreadSummary {
    ThreadSummary {
        id: id.to_owned(),
        title: title.to_owned(),
        tags: Vec::new(),
        workspace_ids: Vec::new(),
        process: ProcessState::Stopped,
        settled: true,
        needs_attention: false,
        provenance: provenance(),
        created_at: 1,
        updated_at: 1,
        archived_at: None,
    }
}

pub fn repository(id: &str) -> Repository {
    Repository {
        id: id.to_owned(),
        name: id.to_lowercase(),
        path: format!("/repos/{id}"),
        default_base_ref: "main".into(),
        remote_url: None,
        created_at: 1,
    }
}
