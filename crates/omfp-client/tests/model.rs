//! L0 tests of the client model: index and thread events, snapshots, replay, resync and change tracking.

mod support;

use omfp_client::Model;
use omfp_client::model::{Applied, ClosedInteraction, Dirty, StartMode, SubscribeError, TopicSync};
use omfp_core::{ProcessState, Resolution, ThreadEvent, ThreadState, replay};
use omfp_protocol::{ErrorCode, IndexEvent, IndexSnapshot, Position, SubscribeResult, Topic};
use serde_json::json;
use support::{FIXTURES, fixture, index_frame, repository, summary, thread_frame, thread_topic};

const THREAD: &str = "01THREAD";

fn snapshot(epoch: &str, seq: u64, state: &impl serde::Serialize) -> SubscribeResult {
    SubscribeResult::Snapshot {
        epoch: epoch.into(),
        seq,
        state: serde_json::to_value(state).unwrap(),
    }
}

/// A model following `thread/<THREAD>` from a snapshot of `events` at `epoch`/`events.len()`.
fn following(epoch: &str, events: &[ThreadEvent]) -> Model {
    let mut model = Model::default();
    let topic = thread_topic(THREAD);
    assert!(model.track(topic.clone()));
    let state = replay(events);
    let started = model.apply_subscribed(&topic, snapshot(epoch, events.len() as u64, &state));
    assert_eq!(started, Ok(StartMode::Snapshot));
    model
}

fn apply_all(model: &mut Model, epoch: &str, first_seq: u64, events: &[ThreadEvent]) {
    for (seq, event) in (first_seq..).zip(events) {
        let frame = thread_frame(THREAD, epoch, seq, event.clone());
        assert_eq!(model.apply_event(&frame), Applied::Applied, "seq {seq}");
    }
}

fn thread_state(model: &Model) -> &ThreadState {
    &model.thread(THREAD).expect("thread followed").state
}

// spec: UI-010, PRT-021
#[test]
fn a_snapshot_plus_the_following_events_equals_the_reducer_over_every_event() {
    for name in FIXTURES {
        let events = fixture(name);
        for split in [0, events.len() / 2, events.len()] {
            let mut model = following("e1", &events[..split]);

            apply_all(&mut model, "e1", split as u64 + 1, &events[split..]);

            assert_eq!(
                thread_state(&model),
                &replay(&events),
                "{name} split at {split}"
            );
            assert_eq!(
                model.topic(&thread_topic(THREAD)).unwrap().position,
                Some(Position {
                    epoch: "e1".into(),
                    seq: events.len() as u64
                })
            );
        }
    }
}

// spec: PRT-020, UI-003
#[test]
fn after_a_connection_loss_the_topic_resubscribes_from_its_position_and_replay_continues() {
    let events = fixture("bash_tool_call");
    let mut model = following("e1", &events[..10]);
    apply_all(&mut model, "e1", 11, &events[10..20]);
    let topic = thread_topic(THREAD);

    model.connection_lost();

    assert_eq!(model.topic(&topic).unwrap().sync, TopicSync::Subscribing);
    // Events of the old connection no longer apply.
    let late = thread_frame(THREAD, "e1", 21, events[20].clone());
    assert_eq!(model.apply_event(&late), Applied::Ignored);
    let params = model.subscribe_params(&topic).unwrap();
    assert_eq!(
        params.since,
        Some(Position {
            epoch: "e1".into(),
            seq: 20
        })
    );
    let replayed = model.apply_subscribed(
        &topic,
        SubscribeResult::Replay {
            epoch: "e1".into(),
            from_seq: 21,
        },
    );
    assert_eq!(replayed, Ok(StartMode::Replay));
    apply_all(&mut model, "e1", 21, &events[20..]);
    assert_eq!(thread_state(&model), &replay(&events));
    assert_eq!(
        model.topic(&topic).unwrap().sync,
        TopicSync::Live(StartMode::Replay)
    );
}

// spec: PRT-020
#[test]
fn a_replay_that_does_not_continue_the_position_asks_for_a_snapshot() {
    let events = fixture("text_reply");
    let topic = thread_topic(THREAD);
    for answer in [
        SubscribeResult::Replay {
            epoch: "e2".into(),
            from_seq: 6,
        },
        SubscribeResult::Replay {
            epoch: "e1".into(),
            from_seq: 9,
        },
    ] {
        let mut model = following("e1", &events[..5]);
        model.connection_lost();

        let result = model.apply_subscribed(&topic, answer);

        assert!(
            matches!(result, Err(SubscribeError::ReplayMismatch { .. })),
            "{result:?}"
        );
        assert_eq!(model.subscribe_params(&topic).unwrap().since, None);
    }
}

// spec: PRT-022
#[test]
fn duplicates_are_ignored_and_a_gap_drops_the_position() {
    let events = fixture("text_reply");
    let mut model = following("e1", &events[..3]);
    let topic = thread_topic(THREAD);
    let before = thread_state(&model).clone();

    let duplicate = thread_frame(THREAD, "e1", 3, events[2].clone());
    assert_eq!(model.apply_event(&duplicate), Applied::Ignored);
    assert_eq!(thread_state(&model), &before);

    let skipped = thread_frame(THREAD, "e1", 5, events[4].clone());
    assert_eq!(model.apply_event(&skipped), Applied::Gap);
    assert_eq!(thread_state(&model), &before);
    assert_eq!(model.topic(&topic).unwrap().sync, TopicSync::Subscribing);
    assert_eq!(model.subscribe_params(&topic).unwrap().since, None);
}

// spec: PRT-022
#[test]
fn a_new_epoch_continues_at_seq_one_and_anything_else_is_a_gap() {
    let events = fixture("restart_with_history");
    let mut model = following("e1", &events[..4]);

    // A new omp process restarts numbering; the daemon's state carries over.
    apply_all(&mut model, "e2", 1, &events[4..]);
    assert_eq!(thread_state(&model), &replay(&events));

    let mut model = following("e1", &events[..4]);
    let jump = thread_frame(THREAD, "e2", 3, events[4].clone());
    assert_eq!(model.apply_event(&jump), Applied::Gap);
}

// spec: PRT-023
#[test]
fn resync_keeps_the_data_until_the_snapshot_replaces_it() {
    let events = fixture("text_reply");
    let mut model = following("e1", &events[..10]);
    let topic = thread_topic(THREAD);
    let stale = thread_state(&model).clone();

    assert!(model.resync(&topic));

    assert_eq!(thread_state(&model), &stale);
    let ignored = thread_frame(THREAD, "e1", 11, events[10].clone());
    assert_eq!(model.apply_event(&ignored), Applied::Ignored);
    assert_eq!(model.subscribe_params(&topic).unwrap().since, None);
    let fresh = replay(&events);
    let started = model.apply_subscribed(&topic, snapshot("e1", events.len() as u64, &fresh));
    assert_eq!(started, Ok(StartMode::Snapshot));
    assert_eq!(thread_state(&model), &fresh);
    assert!(
        !model.resync(&Topic::Index),
        "untracked topics are not resynced"
    );
}

#[test]
fn the_index_is_keyed_and_ordered_by_id_and_follows_its_events() {
    let mut model = Model::default();
    model.track(Topic::Index);
    let state = IndexSnapshot {
        repositories: vec![repository("01R2"), repository("01R1")],
        workspaces: Vec::new(),
        threads: vec![summary("01T2", "second"), summary("01T1", "first")],
    };
    model
        .apply_subscribed(&Topic::Index, snapshot("i1", 7, &state))
        .unwrap();

    let renamed = summary("01T1", "renamed");
    let events = [
        IndexEvent::ThreadUpserted {
            thread: renamed.clone(),
        },
        IndexEvent::ThreadUpserted {
            thread: summary("01T0", "zeroth"),
        },
        IndexEvent::ThreadRemoved {
            thread_id: "01T2".into(),
        },
        IndexEvent::RepositoryRemoved {
            repository_id: "01R2".into(),
        },
    ];
    for (seq, event) in (8..).zip(events) {
        assert_eq!(
            model.apply_event(&index_frame("i1", seq, event)),
            Applied::Applied
        );
    }

    let index = model.index();
    assert_eq!(index.threads.keys().collect::<Vec<_>>(), ["01T0", "01T1"]);
    assert_eq!(index.threads["01T1"], renamed);
    assert_eq!(index.repositories.keys().collect::<Vec<_>>(), ["01R1"]);
}

#[test]
fn a_snapshot_replaces_the_previous_state_and_position() {
    let mut model = Model::default();
    model.track(Topic::Index);
    let first = IndexSnapshot {
        repositories: vec![repository("01R1")],
        workspaces: Vec::new(),
        threads: vec![summary("01T1", "one")],
    };
    model
        .apply_subscribed(&Topic::Index, snapshot("i1", 3, &first))
        .unwrap();
    let second = IndexSnapshot {
        repositories: Vec::new(),
        workspaces: Vec::new(),
        threads: vec![summary("01T9", "nine")],
    };

    model
        .apply_subscribed(&Topic::Index, snapshot("i2", 1, &second))
        .unwrap();

    assert_eq!(model.index().threads.keys().collect::<Vec<_>>(), ["01T9"]);
    assert_eq!(model.index().repositories.len(), 0);
    assert_eq!(
        model.topic(&Topic::Index).unwrap().position,
        Some(Position {
            epoch: "i2".into(),
            seq: 1
        })
    );
}

#[test]
fn events_of_unsubscribed_topics_or_before_the_snapshot_are_ignored() {
    let mut model = Model::default();
    let event = ThreadEvent::Process {
        state: ProcessState::Starting,
    };
    assert_eq!(
        model.apply_event(&thread_frame(THREAD, "e1", 1, event.clone())),
        Applied::Ignored
    );

    model.track(thread_topic(THREAD));
    assert_eq!(
        model.apply_event(&thread_frame(THREAD, "e1", 1, event)),
        Applied::Ignored
    );
    assert_eq!(model.thread(THREAD), None);
}

#[test]
fn an_invalid_snapshot_fails_the_topic() {
    let mut model = Model::default();
    let topic = thread_topic(THREAD);
    model.track(topic.clone());

    let result = model.apply_subscribed(
        &topic,
        SubscribeResult::Snapshot {
            epoch: "e1".into(),
            seq: 1,
            state: json!({ "nope": true }),
        },
    );

    assert!(
        matches!(result, Err(SubscribeError::Snapshot { .. })),
        "{result:?}"
    );
    let TopicSync::Failed(error) = &model.topic(&topic).unwrap().sync else {
        panic!("not failed");
    };
    assert_eq!(error.code, ErrorCode::Internal);
    assert!(model.track(topic), "a failed topic is subscribed again");
}

#[test]
fn unsubscribing_drops_the_topic_and_its_data() {
    let mut model = following("e1", &fixture("text_reply"));
    model.take_dirty();

    assert!(model.untrack(&thread_topic(THREAD)));

    assert_eq!(model.thread(THREAD), None);
    assert_eq!(model.topics(), []);
    let dirty = model.take_dirty();
    assert!(dirty.topics);
    assert_eq!(dirty.threads.into_iter().collect::<Vec<_>>(), [THREAD]);
    assert!(!model.untrack(&thread_topic(THREAD)));
}

#[test]
fn workspace_topics_keep_the_git_status() {
    let mut model = Model::default();
    let topic = Topic::Workspace("01W".into());
    model.track(topic.clone());

    model
        .apply_subscribed(&topic, snapshot("w1", 0, &json!(null)))
        .unwrap();
    assert_eq!(model.workspaces()["01W"], None);

    let frame: omfp_protocol::EventFrame = serde_json::from_value(json!({
        "topic": "workspace/01W", "epoch": "w1", "seq": 1,
        "event": { "kind": "status", "status": { "ahead": 1, "behind": 0, "files": [] } }
    }))
    .unwrap();
    assert_eq!(model.apply_event(&frame), Applied::Applied);
    assert_eq!(
        model.workspaces()["01W"]
            .as_ref()
            .map(|status| status.ahead),
        Some(1)
    );
}

// spec: UI-012
#[test]
fn changes_accumulate_in_one_dirty_set_until_drained() {
    let events = fixture("text_reply");
    let mut model = following("e1", &events[..1]);
    model.track(Topic::Index);
    model.take_dirty();
    let revision = model.revision();

    apply_all(&mut model, "e1", 2, &events[1..]);
    let state = IndexSnapshot {
        repositories: Vec::new(),
        workspaces: Vec::new(),
        threads: Vec::new(),
    };
    model
        .apply_subscribed(&Topic::Index, snapshot("i1", 0, &state))
        .unwrap();

    assert!(model.revision() > revision);
    let dirty = model.take_dirty();
    assert_eq!(
        dirty,
        Dirty {
            connection: false,
            topics: true,
            index: true,
            threads: [THREAD.to_owned()].into(),
            workspaces: [].into(),
        }
    );
    assert!(model.take_dirty().is_empty());
}

#[test]
fn interactions_closed_by_live_events_are_kept_with_their_resolution() {
    for name in ["approval_granted", "ask_answered"] {
        let events = fixture(name);
        let mut model = following("e1", &[]);
        apply_all(&mut model, "e1", 1, &events);

        let view = model.thread(THREAD).unwrap();
        let answered: Vec<&ClosedInteraction> = view
            .closed
            .iter()
            .filter(|closed| closed.resolution.is_some())
            .collect();
        let resolutions = events
            .iter()
            .filter(|event| matches!(event, ThreadEvent::InteractionResolved { .. }))
            .count();
        assert_eq!(answered.len(), resolutions, "{name}");
        assert!(
            answered.iter().all(|closed| closed.resolution
                == Some(Resolution::Client {
                    client: closed_client(&events)
                })),
            "{name}"
        );
        assert_eq!(view.state.interactions, Vec::new(), "{name}");
    }
}

fn closed_client(events: &[ThreadEvent]) -> String {
    events
        .iter()
        .find_map(|event| match event {
            ThreadEvent::InteractionResolved {
                resolution: Resolution::Client { client },
                ..
            } => Some(client.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn a_snapshot_restarts_the_closed_interaction_history() {
    let events = fixture("approval_granted");
    let mut model = following("e1", &[]);
    apply_all(&mut model, "e1", 1, &events);
    assert_ne!(model.thread(THREAD).unwrap().closed, Vec::new());

    model
        .apply_subscribed(&thread_topic(THREAD), snapshot("e2", 1, &replay(&events)))
        .unwrap();

    assert_eq!(model.thread(THREAD).unwrap().closed, Vec::new());
}
