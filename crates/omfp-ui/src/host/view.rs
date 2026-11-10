//! Data in: the [`ViewInput`] kept in step with the client's [`Model`] and with what dispatched intents returned.
//!
//! The model says what changed in a [`Dirty`] set; only those parts of the input are rebuilt, so a streaming thread
//! costs one thread clone per frame (UI-012), not a copy of everything.

use std::collections::BTreeMap;

use omfp_client::model::{ConnectionStatus, Dirty, Model};
use omfp_core::wire::{ModelInfo, SessionState, ThinkingLevel};
use omfp_protocol::GitStatus;

use crate::input::{ClosedInteraction, ConnectionView, OutcomeEntry, ThreadDetails, ViewInput};
use crate::intent::IntentOutcome;

/// Outcomes kept in [`ViewInput::outcomes`]; the UI applies new ones on the next render, so older ones are history.
pub const OUTCOMES_KEPT: usize = 16;

/// What a dispatched intent brought back besides the topics.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Something the UI must be told (errors, created threads, restored queue texts, queue results).
    Outcome(IntentOutcome),
    /// Thread data fetched by calls; `None` fields stay as they are.
    Details {
        thread_id: String,
        session: Option<Box<SessionState>>,
        models: Option<Vec<ModelInfo>>,
        thinking_levels: Option<Vec<ThinkingLevel>>,
    },
    /// A workspace's git status (`None`: not a git repository).
    Git {
        workspace_id: String,
        status: Option<GitStatus>,
    },
}

/// Host-side state behind the input: thread details fetched before the thread's snapshot arrived.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewSync {
    details: BTreeMap<String, ThreadDetails>,
}

impl ViewSync {
    /// Brings `input` up to date with the parts of `model` that `dirty` names.
    pub fn apply_model(&mut self, input: &mut ViewInput, model: &Model, dirty: &Dirty) {
        if dirty.connection {
            input.connection = connection_view(model.connection());
        }
        if dirty.index {
            let index = model.index();
            input.repositories = index.repositories.values().cloned().collect();
            input.workspaces = index.workspaces.values().cloned().collect();
            input.threads = index.threads.values().cloned().collect();
        }
        for thread_id in &dirty.threads {
            let Some(thread) = model.thread(thread_id) else {
                input.open.remove(thread_id);
                self.details.remove(thread_id);
                continue;
            };
            let open =
                input
                    .open
                    .entry(thread_id.clone())
                    .or_insert_with(|| crate::input::ThreadView {
                        details: self.details.get(thread_id).cloned().unwrap_or_default(),
                        ..crate::input::ThreadView::default()
                    });
            open.state.clone_from(&thread.state);
            open.closed = thread
                .closed
                .iter()
                .map(|closed| ClosedInteraction {
                    request: closed.request.clone(),
                    resolution: closed.resolution.clone(),
                })
                .collect();
        }
        for workspace_id in &dirty.workspaces {
            match model.workspaces().get(workspace_id) {
                Some(Some(status)) => {
                    input.git.insert(workspace_id.clone(), status.clone());
                }
                Some(None) => {
                    input.git.remove(workspace_id);
                }
                // Unsubscribed: the last status fetched stays.
                None => {}
            }
        }
    }

    /// Applies what an intent brought back.
    pub fn apply_effect(&mut self, input: &mut ViewInput, effect: Effect) {
        match effect {
            Effect::Outcome(outcome) => {
                let seq = input.outcomes.last().map_or(0, |entry| entry.seq + 1);
                input.outcomes.push(OutcomeEntry { seq, outcome });
                let excess = input.outcomes.len().saturating_sub(OUTCOMES_KEPT);
                input.outcomes.drain(..excess);
            }
            Effect::Details {
                thread_id,
                session,
                models,
                thinking_levels,
            } => {
                let stored = self.details.entry(thread_id.clone()).or_default();
                if let Some(session) = session {
                    stored.session = Some(*session);
                }
                if let Some(models) = models {
                    stored.models = models;
                }
                if let Some(levels) = thinking_levels {
                    stored.thinking_levels = levels;
                }
                if let Some(open) = input.open.get_mut(&thread_id) {
                    open.details.clone_from(stored);
                }
            }
            Effect::Git {
                workspace_id,
                status: Some(status),
            } => {
                input.git.insert(workspace_id, status);
            }
            Effect::Git {
                workspace_id,
                status: None,
            } => {
                input.git.remove(&workspace_id);
            }
        }
    }
}

/// The banner's view of the client's connection status (UI-003).
#[must_use]
pub fn connection_view(status: &ConnectionStatus) -> ConnectionView {
    match status {
        ConnectionStatus::Connecting => ConnectionView::Connecting,
        ConnectionStatus::Connected(_) => ConnectionView::Connected,
        ConnectionStatus::Reconnecting {
            attempt, retry_in, ..
        } => ConnectionView::Reconnecting {
            attempt: *attempt,
            retry_in_ms: u64::try_from(retry_in.as_millis()).unwrap_or(u64::MAX),
        },
        ConnectionStatus::Unauthenticated => ConnectionView::Unauthenticated,
        ConnectionStatus::VersionMismatch { message, .. } => ConnectionView::Failed {
            message: message.clone(),
        },
        ConnectionStatus::Closed => ConnectionView::Failed {
            message: "the connection was closed".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use omfp_core::ThreadEvent;
    use omfp_protocol::{
        EventFrame, HelloResult, IndexEvent, IndexSnapshot, SubscribeResult, Topic, TopicEvent,
    };

    use super::*;
    use crate::input::ThreadView;
    use crate::test_support::{fixture_events, repository, thread, workspace};

    fn sync_once(sync: &mut ViewSync, input: &mut ViewInput, model: &mut Model) {
        let dirty = model.take_dirty();
        sync.apply_model(input, model, &dirty);
    }

    fn snapshot(model: &mut Model, topic: &Topic, state: serde_json::Value) {
        model.track(topic.clone());
        model
            .apply_subscribed(
                topic,
                SubscribeResult::Snapshot {
                    epoch: "e1".into(),
                    seq: 0,
                    state,
                },
            )
            .unwrap();
    }

    fn index_model() -> Model {
        let mut model = Model::default();
        let index = IndexSnapshot {
            repositories: vec![repository("r1", "front")],
            workspaces: vec![
                workspace("w2", Some("r1"), None),
                workspace("w1", None, None),
            ],
            threads: vec![thread("t1", "one", &["w1"])],
        };
        snapshot(
            &mut model,
            &Topic::Index,
            serde_json::to_value(index).unwrap(),
        );
        model
    }

    // spec: UI-003
    #[test]
    fn the_connection_status_maps_to_the_banner_view() {
        let hello = HelloResult {
            protocol: 1,
            daemon: "0.1.0".into(),
            omp: "18.18.6".into(),
            warnings: Vec::new(),
        };
        assert_eq!(
            connection_view(&ConnectionStatus::Connected(hello)),
            ConnectionView::Connected
        );
        assert_eq!(
            connection_view(&ConnectionStatus::Reconnecting {
                attempt: 3,
                retry_in: Duration::from_secs(2),
                error: "refused".into(),
            }),
            ConnectionView::Reconnecting {
                attempt: 3,
                retry_in_ms: 2000
            }
        );
        assert_eq!(
            connection_view(&ConnectionStatus::Unauthenticated),
            ConnectionView::Unauthenticated
        );
        assert_eq!(
            connection_view(&ConnectionStatus::VersionMismatch {
                supported: vec![2],
                message: "protocol 1 is not supported".into(),
            }),
            ConnectionView::Failed {
                message: "protocol 1 is not supported".into()
            }
        );
    }

    // spec: UI-012
    #[test]
    fn only_the_dirty_parts_of_the_input_are_rebuilt() {
        let mut model = index_model();
        let mut input = ViewInput::default();
        let mut sync = ViewSync::default();
        sync_once(&mut sync, &mut input, &mut model);
        assert_eq!(
            input
                .workspaces
                .iter()
                .map(|workspace| workspace.id.as_str())
                .collect::<Vec<_>>(),
            vec!["w1", "w2"],
            "index order is id order"
        );
        assert_eq!(input.threads, vec![thread("t1", "one", &["w1"])]);

        // A change the dirty set does not name is left alone.
        input.threads.clear();
        model.set_connection(ConnectionStatus::Unauthenticated);
        sync_once(&mut sync, &mut input, &mut model);
        assert_eq!(input.connection, ConnectionView::Unauthenticated);
        assert_eq!(input.threads, Vec::new());
    }

    // spec: UI-010
    #[test]
    fn subscribed_threads_follow_the_reducer_and_leave_when_unsubscribed() {
        let mut model = Model::default();
        let topic = Topic::Thread("t1".into());
        snapshot(
            &mut model,
            &topic,
            serde_json::to_value(omfp_core::ThreadState::default()).unwrap(),
        );
        let mut input = ViewInput::default();
        let mut sync = ViewSync::default();
        sync_once(&mut sync, &mut input, &mut model);
        assert_eq!(input.open["t1"].state, omfp_core::ThreadState::default());

        let events: Vec<ThreadEvent> = fixture_events("text_reply");
        for (seq, event) in events.iter().enumerate() {
            let frame = EventFrame {
                topic: topic.clone(),
                epoch: "e1".into(),
                seq: seq as u64 + 1,
                event: TopicEvent::Thread(event.clone()),
            };
            model.apply_event(&frame);
        }
        sync_once(&mut sync, &mut input, &mut model);
        assert_eq!(input.open["t1"].state, omfp_core::replay(&events));

        model.untrack(&topic);
        sync_once(&mut sync, &mut input, &mut model);
        assert_eq!(input.open.get("t1"), None);
    }

    #[test]
    fn details_fetched_before_the_snapshot_wait_for_it() {
        let mut input = ViewInput::default();
        let mut sync = ViewSync::default();
        let levels = vec![ThinkingLevel::Low, ThinkingLevel::High];
        sync.apply_effect(
            &mut input,
            Effect::Details {
                thread_id: "t1".into(),
                session: None,
                models: None,
                thinking_levels: Some(levels.clone()),
            },
        );
        assert_eq!(
            input.open.get("t1"),
            None,
            "no empty thread before its snapshot"
        );

        let mut model = Model::default();
        snapshot(
            &mut model,
            &Topic::Thread("t1".into()),
            serde_json::to_value(omfp_core::ThreadState::default()).unwrap(),
        );
        sync_once(&mut sync, &mut input, &mut model);
        assert_eq!(input.open["t1"].details.thinking_levels, levels);

        // Later details merge field by field.
        sync.apply_effect(
            &mut input,
            Effect::Details {
                thread_id: "t1".into(),
                session: None,
                models: Some(Vec::new()),
                thinking_levels: None,
            },
        );
        assert_eq!(
            input.open["t1"],
            ThreadView {
                details: ThreadDetails {
                    thinking_levels: levels,
                    ..ThreadDetails::default()
                },
                ..ThreadView::default()
            }
        );
    }

    #[test]
    fn outcomes_get_increasing_sequence_numbers_and_old_ones_are_dropped() {
        let mut input = ViewInput::default();
        let mut sync = ViewSync::default();
        for index in 0..OUTCOMES_KEPT + 2 {
            sync.apply_effect(
                &mut input,
                Effect::Outcome(IntentOutcome::ThreadCreated {
                    thread_id: format!("t{index}"),
                }),
            );
        }
        let seqs: Vec<u64> = input.outcomes.iter().map(|entry| entry.seq).collect();
        assert_eq!(seqs, (2..OUTCOMES_KEPT as u64 + 2).collect::<Vec<_>>());
    }

    #[test]
    fn git_statuses_come_from_workspace_topics_and_refreshes() {
        let status = GitStatus {
            branch: Some("feat".into()),
            base_ref: None,
            ahead: 1,
            behind: 0,
            files: Vec::new(),
        };
        let mut input = ViewInput::default();
        let mut sync = ViewSync::default();
        sync.apply_effect(
            &mut input,
            Effect::Git {
                workspace_id: "w1".into(),
                status: Some(status.clone()),
            },
        );
        assert_eq!(input.git.get("w1"), Some(&status));

        let mut model = index_model();
        let topic = Topic::Workspace("w1".into());
        snapshot(&mut model, &topic, serde_json::Value::Null);
        sync_once(&mut sync, &mut input, &mut model);
        assert_eq!(input.git.get("w1"), None, "not a git repository");

        let index_event = EventFrame {
            topic: Topic::Index,
            epoch: "e1".into(),
            seq: 1,
            event: TopicEvent::Index(IndexEvent::ThreadRemoved {
                thread_id: "t1".into(),
            }),
        };
        model.apply_event(&index_event);
        sync_once(&mut sync, &mut input, &mut model);
        assert_eq!(input.threads, Vec::new());
    }
}
