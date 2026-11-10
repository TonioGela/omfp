//! L1 tests of the session against an in-memory transport and a manual timer: hello, correlation, subscriptions,
//! reconnect backoff, resubscription positions, resync and the stop conditions.

mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use futures::executor::LocalPool;
use futures::task::LocalSpawnExt;
use omfp_client::CallError;
use omfp_client::model::{ConnectionStatus, StartMode, TopicSync};
use omfp_core::{ProcessState, ThreadState};
use omfp_protocol::{Call, Empty, HelloResult, Position, ThreadRef, Topic};
use serde_json::{Value, json};
use support::fake::{Rig, ServerEnd, TOKEN, hello_result};
use support::summary;

const THREAD: &str = "01THREAD";

fn thread_topic() -> Topic {
    Topic::Thread(THREAD.into())
}

fn repos_list() -> Call {
    Call::ReposList(Empty {})
}

fn thread_state_call() -> Call {
    Call::ThreadState(ThreadRef {
        thread_id: THREAD.into(),
    })
}

fn snapshot(epoch: &str, seq: u64, state: &Value) -> Value {
    json!({ "mode": "snapshot", "epoch": epoch, "seq": seq, "state": state })
}

fn empty_index() -> Value {
    json!({ "repositories": [], "workspaces": [], "threads": [] })
}

fn thread_snapshot() -> Value {
    serde_json::to_value(ThreadState::default()).unwrap()
}

fn index_event(epoch: &str, seq: u64, id: &str) -> Value {
    json!({
        "type": "event", "topic": "index", "epoch": epoch, "seq": seq,
        "event": { "kind": "thread_upserted", "thread": summary(id, id) }
    })
}

fn thread_event(epoch: &str, seq: u64, state: &str) -> Value {
    json!({
        "type": "event", "topic": format!("thread/{THREAD}"), "epoch": epoch, "seq": seq,
        "event": { "kind": "process", "state": { "state": state } }
    })
}

fn subscribes(frames: &[Value]) -> Vec<Value> {
    frames
        .iter()
        .filter(|frame| frame["method"] == "subscribe")
        .map(|frame| frame["params"].clone())
        .collect()
}

fn find<'a>(frames: &'a [Value], topic: &str) -> &'a Value {
    frames
        .iter()
        .find(|frame| frame["params"]["topic"] == topic)
        .unwrap_or_else(|| panic!("no request for {topic} in {frames:?}"))
}

fn sync(rig: &Rig, topic: &Topic) -> TopicSync {
    rig.client
        .read(|model| model.topic(topic).map(|state| state.sync.clone()))
        .expect("topic tracked")
}

/// A rig connected and following the index (`i1`/2) and the thread (`e1`/3).
fn following() -> (Rig, ServerEnd) {
    let mut rig = Rig::new();
    rig.client.subscribe(Topic::Index);
    rig.client.subscribe(thread_topic());
    let (server, frames) = rig.connect();
    server.respond(
        &find(&frames, "index")["id"],
        &snapshot("i1", 2, &empty_index()),
    );
    let thread = find(&frames, &format!("thread/{THREAD}"));
    server.respond(&thread["id"], &snapshot("e1", 3, &thread_snapshot()));
    rig.run();
    (rig, server)
}

/// Drops the connection and lets the first retry connect.
fn reconnect(rig: &mut Rig, server: &ServerEnd) -> (ServerEnd, Vec<Value>) {
    server.close(Some(1001));
    rig.run();
    rig.timer.fire();
    rig.run();
    rig.connect()
}

// spec: PRT-001, PRT-002
#[test]
fn the_first_frame_is_hello_with_the_token_and_its_answer_connects() {
    let mut rig = Rig::new();
    let mut server = rig.accept();

    let frames = server.frames();

    assert_eq!(
        frames,
        [json!({
            "type": "request", "id": frames[0]["id"], "method": "system.hello",
            "params": { "token": TOKEN, "protocol": omfp_protocol::PROTOCOL_VERSION, "client": "test/l1" }
        })]
    );
    assert_eq!(rig.status(), ConnectionStatus::Connecting);
    server.respond(&frames[0]["id"], &hello_result());
    rig.run();
    let hello: HelloResult = serde_json::from_value(hello_result()).unwrap();
    assert_eq!(rig.status(), ConnectionStatus::Connected(hello));
}

// spec: UI-003
#[test]
fn reconnect_attempts_back_off_exponentially_up_to_ten_seconds() {
    let mut rig = Rig::new();
    let expected: Vec<Duration> = [500, 1000, 2000, 4000, 8000, 10_000, 10_000, 10_000]
        .into_iter()
        .map(Duration::from_millis)
        .collect();

    for (attempt, retry_in) in (1..).zip(&expected) {
        rig.net
            .take_attempt()
            .expect("an attempt")
            .refuse("connection refused");
        rig.run();
        assert_eq!(
            rig.status(),
            ConnectionStatus::Reconnecting {
                attempt,
                retry_in: *retry_in,
                error: "connection refused".into()
            }
        );
        assert_eq!(
            rig.net.attempts(),
            attempt as usize,
            "no attempt before the delay"
        );
        rig.timer.fire();
        rig.run();
    }

    assert_eq!(rig.timer.requested(), expected);
    assert_eq!(rig.net.attempts(), expected.len() + 1);
}

// spec: UI-003
#[test]
fn the_backoff_starts_over_after_a_successful_connection() {
    let mut rig = Rig::new();
    for _ in 0..2 {
        rig.net.take_attempt().unwrap().refuse("connection refused");
        rig.run();
        rig.timer.fire();
        rig.run();
    }
    let (server, _) = rig.connect();

    server.close(None);
    rig.run();

    let ConnectionStatus::Reconnecting {
        attempt, retry_in, ..
    } = rig.status()
    else {
        panic!("not reconnecting: {:?}", rig.status());
    };
    assert_eq!((attempt, retry_in), (1, Duration::from_millis(500)));
}

// spec: UI-003, PRT-020
#[test]
fn after_a_reconnect_every_topic_is_resubscribed_from_its_last_position() {
    let (mut rig, server) = following();
    server.send(&index_event("i1", 3, "01NEW"));
    server.send(&thread_event("e1", 4, "starting"));
    rig.run();

    let (server, frames) = reconnect(&mut rig, &server);

    assert_eq!(
        subscribes(&frames),
        [
            json!({ "topic": "index", "since": { "epoch": "i1", "seq": 3 } }),
            json!({ "topic": format!("thread/{THREAD}"), "since": { "epoch": "e1", "seq": 4 } }),
        ]
    );
    server.respond(
        &find(&frames, "index")["id"],
        &json!({ "mode": "replay", "epoch": "i1", "fromSeq": 4 }),
    );
    let thread = find(&frames, &format!("thread/{THREAD}"));
    server.respond(&thread["id"], &snapshot("e2", 1, &thread_snapshot()));
    server.send(&index_event("i1", 4, "01NEWER"));
    rig.run();
    assert_eq!(
        sync(&rig, &Topic::Index),
        TopicSync::Live(StartMode::Replay)
    );
    assert_eq!(
        sync(&rig, &thread_topic()),
        TopicSync::Live(StartMode::Snapshot)
    );
    let threads: Vec<String> = rig
        .client
        .read(|model| model.index().threads.keys().cloned().collect());
    assert_eq!(threads, ["01NEW", "01NEWER"]);
    let position = rig
        .client
        .read(|model| model.topic(&thread_topic()).unwrap().position.clone());
    assert_eq!(
        position,
        Some(Position {
            epoch: "e2".into(),
            seq: 1
        })
    );
}

// spec: UI-003
#[test]
fn events_between_the_loss_and_the_resubscription_are_not_applied() {
    let (mut rig, server) = following();
    server.close(None);
    rig.run();
    assert_eq!(sync(&rig, &Topic::Index), TopicSync::Subscribing);
    rig.timer.fire();
    rig.run();
    let (server, frames) = rig.connect();

    // Arrives before the subscribe answer: not part of a known stream yet.
    server.send(&index_event("i1", 3, "01EARLY"));
    rig.run();

    let count = rig.client.read(|model| model.index().threads.len());
    assert_eq!(count, 0);
    assert_eq!(subscribes(&frames).len(), 2);
}

// spec: PRT-023
#[test]
fn resync_required_resubscribes_without_position_and_takes_the_snapshot() {
    let (mut rig, mut server) = following();

    server.send(&json!({ "type": "resync_required", "topic": format!("thread/{THREAD}") }));
    rig.run();

    let frames = server.frames();
    assert_eq!(
        subscribes(&frames),
        [json!({ "topic": format!("thread/{THREAD}") })]
    );
    let state = ThreadState {
        process: ProcessState::Running,
        ..ThreadState::default()
    };
    server.respond(
        &frames[0]["id"],
        &snapshot("e1", 40, &serde_json::to_value(&state).unwrap()),
    );
    rig.run();
    let followed = rig
        .client
        .read(|model| model.thread(THREAD).unwrap().state.clone());
    assert_eq!(followed, state);
}

// spec: PRT-022
#[test]
fn a_gap_in_the_stream_resubscribes_without_position() {
    let (mut rig, mut server) = following();

    server.send(&thread_event("e1", 6, "starting"));
    rig.run();

    assert_eq!(
        subscribes(&server.frames()),
        [json!({ "topic": format!("thread/{THREAD}") })]
    );
    assert_eq!(sync(&rig, &thread_topic()), TopicSync::Subscribing);
}

// spec: PRT-001, UI-003
#[test]
fn a_refused_token_stops_the_client_without_retrying() {
    for answer_first in [true, false] {
        let mut rig = Rig::new();
        let pending = rig.call(repos_list());
        let mut server = rig.accept();
        let hello = server.frames().remove(0);

        if answer_first {
            server.fail(
                &hello["id"],
                &json!({ "code": "unauthorized", "message": "invalid token" }),
            );
        }
        server.close(Some(4401));
        rig.run();

        assert_eq!(rig.status(), ConnectionStatus::Unauthenticated);
        assert!(rig.driver_done());
        assert_eq!(rig.timer.requested(), Vec::<Duration>::new());
        assert_eq!(rig.net.attempts(), 1);
        assert_eq!(*pending.borrow(), Some(Err(CallError::Closed)));
    }
}

// spec: PRT-004
#[test]
fn a_refused_protocol_version_is_reported_and_not_retried() {
    let mut rig = Rig::new();
    let mut server = rig.accept();
    let hello = server.frames().remove(0);

    server.fail(
        &hello["id"],
        &json!({ "code": "version_mismatch", "message": "unsupported", "details": { "supported": [1] } }),
    );
    server.close(Some(4426));
    rig.run();

    assert_eq!(
        rig.status(),
        ConnectionStatus::VersionMismatch {
            supported: vec![1],
            message: "unsupported".into()
        }
    );
    assert!(rig.driver_done());
    assert_eq!(rig.net.attempts(), 1);
}

// spec: PRT-010
#[test]
fn responses_are_matched_to_calls_by_id_in_any_order() {
    let mut rig = Rig::new();
    let (mut server, _) = rig.connect();
    let first = rig.call(repos_list());
    let second = rig.call(thread_state_call());

    let frames = server.frames();
    assert_eq!(frames.len(), 2);
    assert_ne!(frames[0]["id"], frames[1]["id"]);
    server.respond(&frames[1]["id"], &json!("state"));
    server.fail(
        &frames[0]["id"],
        &json!({ "code": "internal", "message": "boom" }),
    );
    rig.run();

    assert_eq!(*second.borrow(), Some(Ok(json!("state"))));
    let Some(Err(CallError::Daemon(error))) = first.borrow().clone() else {
        panic!("not a daemon error: {:?}", first.borrow());
    };
    assert_eq!(error.message, "boom");
}

#[test]
fn calls_made_offline_are_sent_once_connected_and_in_flight_calls_fail_on_loss() {
    let mut rig = Rig::new();
    let queued = rig.call(repos_list());
    let (mut server, frames) = rig.connect();
    assert_eq!(
        frames
            .iter()
            .map(|frame| frame["method"].clone())
            .collect::<Vec<_>>(),
        ["repos.list"]
    );
    server.respond(&frames[0]["id"], &json!({ "repositories": [] }));
    let in_flight = rig.call(thread_state_call());
    assert_eq!(server.frames().len(), 1);

    server.close(None);
    rig.run();

    assert_eq!(*queued.borrow(), Some(Ok(json!({ "repositories": [] }))));
    assert_eq!(*in_flight.borrow(), Some(Err(CallError::ConnectionLost)));
}

#[test]
fn typed_requests_decode_their_result() {
    let mut rig = Rig::new();
    let (mut server, _) = rig.connect();
    let result = Rc::new(std::cell::RefCell::new(None));
    let slot = Rc::clone(&result);
    let request = rig
        .client
        .request::<omfp_protocol::method::ReposList>(Empty {});
    let mut pool = LocalPool::new();
    pool.spawner()
        .spawn_local(async move { *slot.borrow_mut() = Some(request.await) })
        .unwrap();
    rig.run();
    let frames = server.frames();

    server.respond(&frames[0]["id"], &json!({ "repositories": [] }));
    rig.run();
    pool.run_until_stalled();

    assert_eq!(
        *result.borrow(),
        Some(Ok(omfp_protocol::RepositoryList {
            repositories: Vec::new()
        }))
    );
}

#[test]
fn session_methods_cannot_be_called_directly() {
    let mut rig = Rig::new();
    let subscribe = Call::Unsubscribe(omfp_protocol::TopicParams {
        topic: Topic::Index,
    });

    let result = rig.call(subscribe);

    assert_eq!(
        *result.borrow(),
        Some(Err(CallError::Reserved("unsubscribe")))
    );
}

// spec: PRT-024
#[test]
fn unsubscribing_tells_the_daemon_and_drops_the_topic() {
    let (mut rig, mut server) = following();

    rig.client.unsubscribe(thread_topic());
    rig.run();

    let frames = server.frames();
    assert_eq!(frames[0]["method"], "unsubscribe");
    assert_eq!(
        frames[0]["params"],
        json!({ "topic": format!("thread/{THREAD}") })
    );
    assert!(rig.client.read(|model| model.thread(THREAD).is_none()));
    let (_, frames) = reconnect(&mut rig, &server);
    assert_eq!(
        subscribes(&frames),
        [json!({ "topic": "index", "since": { "epoch": "i1", "seq": 2 } })]
    );
}

#[test]
fn closing_the_client_closes_the_connection_and_stops_retrying() {
    let (mut rig, mut server) = following();

    rig.client.close();
    rig.run();

    assert_eq!(rig.status(), ConnectionStatus::Closed);
    assert!(rig.driver_done());
    assert!(server.closed_by_client());
    assert_eq!(rig.timer.requested(), Vec::<Duration>::new());
    assert_eq!(
        *rig.call(repos_list()).borrow(),
        Some(Err(CallError::Closed))
    );
}

// spec: UI-012
#[test]
fn many_changes_wake_the_host_once() {
    let (mut rig, server) = following();
    let wakeups = Rc::new(Cell::new(0));
    let counter = Rc::clone(&wakeups);
    let mut changes = rig.client.changes();
    let mut pool = LocalPool::new();
    pool.spawner()
        .spawn_local(async move {
            loop {
                changes.next().await;
                counter.set(counter.get() + 1);
            }
        })
        .unwrap();
    pool.run_until_stalled();

    for seq in 3..23 {
        server.send(&index_event("i1", seq, &format!("01T{seq:02}")));
    }
    rig.run();
    pool.run_until_stalled();

    assert_eq!(wakeups.get(), 1);
    let dirty = rig.client.take_dirty();
    assert!(dirty.index);
    assert!(rig.client.take_dirty().is_empty());
}

fn methods(frames: &[Value]) -> Vec<String> {
    frames
        .iter()
        .map(|frame| frame["method"].as_str().unwrap().to_owned())
        .collect()
}

fn reconnecting(rig: &Rig) -> (u32, Duration, String) {
    let ConnectionStatus::Reconnecting {
        attempt,
        retry_in,
        error,
    } = rig.status()
    else {
        panic!("not reconnecting: {:?}", rig.status());
    };
    (attempt, retry_in, error)
}

// spec: UI-003
#[test]
fn queued_calls_a_failed_send_left_behind_go_out_on_the_next_connection() {
    let mut rig = Rig::new();
    let mut server = rig.accept();
    let hello = server.frames().remove(0);
    let first = rig.call(repos_list());
    let second = rig.call(thread_state_call());
    let third = rig.call(Call::ThreadRestart(ThreadRef {
        thread_id: THREAD.into(),
    }));
    server.fail_sends_after(1);

    server.respond(&hello["id"], &hello_result());
    rig.run();

    assert_eq!(methods(&server.frames()), ["repos.list"]);
    assert_eq!(reconnecting(&rig).0, 1);
    assert_eq!(*first.borrow(), Some(Err(CallError::ConnectionLost)));
    rig.timer.fire();
    rig.run();
    let (server, frames) = rig.connect();
    assert_eq!(methods(&frames), ["thread.state", "thread.restart"]);
    for frame in &frames {
        server.respond(&frame["id"], &frame["method"]);
    }
    rig.run();
    assert_eq!(*second.borrow(), Some(Ok(json!("thread.state"))));
    assert_eq!(*third.borrow(), Some(Ok(json!("thread.restart"))));
}

// spec: UI-003
#[test]
fn a_call_whose_send_fails_mid_session_is_sent_after_reconnecting() {
    let mut rig = Rig::new();
    let (server, _) = rig.connect();
    server.fail_sends_after(0);

    let call = rig.call(repos_list());

    assert_eq!(reconnecting(&rig).0, 1);
    assert_eq!(*call.borrow(), None);
    rig.timer.fire();
    rig.run();
    let (server, frames) = rig.connect();
    assert_eq!(methods(&frames), ["repos.list"]);
    server.respond(&frames[0]["id"], &json!({ "repositories": [] }));
    rig.run();
    assert_eq!(*call.borrow(), Some(Ok(json!({ "repositories": [] }))));
}

#[test]
fn a_malformed_response_fails_its_call() {
    let mut rig = Rig::new();
    let (mut server, _) = rig.connect();
    let call = rig.call(repos_list());
    let id = server.frames()[0]["id"].clone();

    server.send(&json!({
        "type": "response", "id": id, "ok": true,
        "error": { "code": "internal", "message": "both" }
    }));
    rig.run();

    assert!(
        matches!(*call.borrow(), Some(Err(CallError::Codec(_)))),
        "{:?}",
        call.borrow()
    );
    assert!(matches!(rig.status(), ConnectionStatus::Connected(_)));
}

// spec: UI-003
#[test]
fn a_malformed_hello_answer_reconnects_with_backoff() {
    let mut rig = Rig::new();
    let mut server = rig.accept();
    let hello = server.frames().remove(0);

    server.send(&json!({ "type": "response", "id": hello["id"], "ok": false }));
    rig.run();

    let (attempt, retry_in, _) = reconnecting(&rig);
    assert_eq!((attempt, retry_in), (1, Duration::from_millis(500)));
    assert!(!rig.driver_done());
}

// spec: PRT-022
#[test]
fn a_malformed_event_of_a_followed_topic_resubscribes_it_at_once() {
    let (mut rig, mut server) = following();

    server.send(&json!({
        "type": "event", "topic": format!("thread/{THREAD}"), "epoch": "e1", "seq": 4,
        "event": { "kind": "not_a_thread_event" }
    }));
    rig.run();

    assert_eq!(
        subscribes(&server.frames()),
        [json!({ "topic": format!("thread/{THREAD}") })]
    );
    assert_eq!(sync(&rig, &thread_topic()), TopicSync::Subscribing);
}

#[test]
fn commands_given_during_the_hello_are_sent_once_after_it() {
    let mut rig = Rig::new();
    let mut server = rig.accept();
    let hello = server.frames().remove(0);

    rig.client.subscribe(Topic::Index);
    rig.client.subscribe(thread_topic());
    rig.client.unsubscribe(thread_topic());
    let call = rig.call(repos_list());
    assert_eq!(server.frames(), Vec::<Value>::new());
    server.respond(&hello["id"], &hello_result());
    rig.run();

    let frames = server.frames();
    assert_eq!(methods(&frames), ["subscribe", "repos.list"]);
    assert_eq!(frames[0]["params"], json!({ "topic": "index" }));
    server.respond(&frames[1]["id"], &json!({ "repositories": [] }));
    rig.run();
    assert_eq!(*call.borrow(), Some(Ok(json!({ "repositories": [] }))));
    assert_eq!(server.frames(), Vec::<Value>::new());
}

// spec: PRT-020
#[test]
fn a_replay_that_does_not_continue_the_position_resubscribes_without_it() {
    let (mut rig, server) = following();
    let (mut server, frames) = reconnect(&mut rig, &server);

    server.respond(
        &find(&frames, "index")["id"],
        &json!({ "mode": "replay", "epoch": "i1", "fromSeq": 9 }),
    );
    rig.run();

    assert_eq!(subscribes(&server.frames()), [json!({ "topic": "index" })]);
    assert_eq!(sync(&rig, &Topic::Index), TopicSync::Subscribing);
}

#[test]
fn an_answer_to_a_superseded_subscribe_is_discarded() {
    let mut rig = Rig::new();
    let (mut server, _) = rig.connect();
    rig.client.subscribe(thread_topic());
    rig.client.unsubscribe(thread_topic());
    rig.client.subscribe(thread_topic());
    rig.run();
    let frames = server.frames();
    assert_eq!(methods(&frames), ["subscribe", "unsubscribe", "subscribe"]);

    let running = ThreadState {
        process: ProcessState::Running,
        ..ThreadState::default()
    };
    server.respond(
        &frames[0]["id"],
        &snapshot("e1", 5, &serde_json::to_value(&running).unwrap()),
    );
    server.respond(&frames[1]["id"], &json!({}));
    rig.run();
    assert_eq!(sync(&rig, &thread_topic()), TopicSync::Subscribing);
    assert!(rig.client.read(|model| model.thread(THREAD).is_none()));

    server.respond(&frames[2]["id"], &snapshot("e1", 6, &thread_snapshot()));
    rig.run();
    let state = rig
        .client
        .read(|model| model.thread(THREAD).unwrap().state.clone());
    assert_eq!(state, ThreadState::default());
}

#[test]
fn a_refused_subscription_is_retried_on_request_and_on_the_next_connection() {
    let mut rig = Rig::new();
    let (mut server, _) = rig.connect();
    let not_found = json!({ "code": "not_found", "message": "no such thread" });
    rig.client.subscribe(thread_topic());
    rig.run();
    let id = server.frames()[0]["id"].clone();
    server.fail(&id, &not_found);
    rig.run();
    assert!(matches!(sync(&rig, &thread_topic()), TopicSync::Failed(_)));

    rig.client.subscribe(thread_topic());
    rig.run();
    let frames = server.frames();
    assert_eq!(
        subscribes(&frames),
        [json!({ "topic": format!("thread/{THREAD}") })]
    );
    server.fail(&frames[0]["id"], &not_found);
    rig.run();

    let (_, frames) = reconnect(&mut rig, &server);
    assert_eq!(
        subscribes(&frames),
        [json!({ "topic": format!("thread/{THREAD}") })]
    );
}

// spec: PRT-023
#[test]
fn resync_required_while_resubscribing_waits_for_the_pending_answer() {
    let (mut rig, mut server) = following();
    server.send(&thread_event("e1", 6, "starting"));
    rig.run();
    let frames = server.frames();
    assert_eq!(subscribes(&frames).len(), 1);

    server.send(&json!({ "type": "resync_required", "topic": format!("thread/{THREAD}") }));
    rig.run();

    assert_eq!(server.frames(), Vec::<Value>::new());
    server.respond(&frames[0]["id"], &snapshot("e1", 9, &thread_snapshot()));
    rig.run();
    assert_eq!(
        sync(&rig, &thread_topic()),
        TopicSync::Live(StartMode::Snapshot)
    );
}

// spec: PRT-004
#[test]
fn close_4426_without_an_answer_reports_a_version_mismatch() {
    let mut rig = Rig::new();
    let server = rig.accept();

    server.close(Some(4426));
    rig.run();

    assert_eq!(
        rig.status(),
        ConnectionStatus::VersionMismatch {
            supported: Vec::new(),
            message: String::new()
        }
    );
    assert!(rig.driver_done());
    assert_eq!(rig.net.attempts(), 1);
}

// spec: UI-003
#[test]
fn a_hello_refused_for_another_reason_is_retried_with_backoff() {
    let mut rig = Rig::new();
    let mut server = rig.accept();
    let hello = server.frames().remove(0);

    server.fail(
        &hello["id"],
        &json!({ "code": "internal", "message": "starting up" }),
    );
    rig.run();

    assert_eq!(
        reconnecting(&rig),
        (1, Duration::from_millis(500), "starting up".to_owned())
    );
    assert!(!rig.driver_done());
    rig.timer.fire();
    rig.run();
    assert_eq!(rig.net.attempts(), 2);
}
