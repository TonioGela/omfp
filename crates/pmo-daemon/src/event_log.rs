//! Per-topic event log (DMN-040, DMN-041): numbers events, folds them into a state with the shared reducer, keeps a
//! bounded tail for catch-up, and fans them out to subscribers without gaps (PRT-020..023).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::broadcast;

/// An event with its position.
#[derive(Debug, Clone, PartialEq)]
pub struct Sequenced<E> {
    pub epoch: Arc<str>,
    pub seq: u64,
    pub event: E,
}

/// A position a client already has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub epoch: String,
    pub seq: u64,
}

/// How a subscription starts (PRT-020).
#[derive(Debug, Clone, PartialEq)]
pub enum Start<E, S> {
    /// The missed events, in order; live events follow.
    Replay(Vec<Sequenced<E>>),
    /// The state after `seq` of `epoch`; live events after `seq` follow.
    Snapshot { epoch: Arc<str>, seq: u64, state: S },
}

/// A new subscription: how to start, then the live stream.
#[derive(Debug)]
pub struct Subscription<E, S> {
    pub start: Start<E, S>,
    pub live: broadcast::Receiver<Arc<Sequenced<E>>>,
}

struct Inner<E, S> {
    epoch: Arc<str>,
    last_seq: u64,
    tail: VecDeque<Arc<Sequenced<E>>>,
    state: S,
}

/// The log of one topic. Cheap to share behind an `Arc`.
pub struct EventLog<E, S> {
    inner: Mutex<Inner<E, S>>,
    capacity: usize,
    apply: fn(&mut S, &E),
    sender: broadcast::Sender<Arc<Sequenced<E>>>,
}

impl<E, S> std::fmt::Debug for EventLog<E, S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventLog")
            .field("capacity", &self.capacity)
            .finish_non_exhaustive()
    }
}

impl<E: Clone, S: Clone> EventLog<E, S> {
    /// `capacity` events are kept for replay (DMN-041); `apply` is the reducer; `live_buffer` bounds how far a
    /// subscriber may fall behind before it must resynchronize (PRT-023).
    pub fn new(
        epoch: impl Into<Arc<str>>,
        state: S,
        apply: fn(&mut S, &E),
        capacity: usize,
        live_buffer: usize,
    ) -> Self {
        let (sender, _) = broadcast::channel(live_buffer.max(1));
        Self {
            inner: Mutex::new(Inner {
                epoch: epoch.into(),
                last_seq: 0,
                tail: VecDeque::with_capacity(capacity),
                state,
            }),
            capacity,
            apply,
            sender,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner<E, S>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Applies `event` to the state, numbers it, keeps it for replay and broadcasts it. Returns its `seq`.
    pub fn push(&self, event: E) -> u64 {
        let mut inner = self.lock();
        (self.apply)(&mut inner.state, &event);
        inner.last_seq += 1;
        let sequenced = Arc::new(Sequenced {
            epoch: Arc::clone(&inner.epoch),
            seq: inner.last_seq,
            event,
        });
        if inner.tail.len() == self.capacity {
            inner.tail.pop_front();
        }
        if self.capacity > 0 {
            inner.tail.push_back(Arc::clone(&sequenced));
        }
        // Sending under the lock keeps subscribe() gap-free; no receivers is fine.
        let _ = self.sender.send(sequenced);
        inner.last_seq
    }

    /// Starts a new epoch (a new process): numbering restarts and old positions can no longer be replayed. The state
    /// carries over.
    pub fn new_epoch(&self, epoch: impl Into<Arc<str>>) {
        let mut inner = self.lock();
        inner.epoch = epoch.into();
        inner.last_seq = 0;
        inner.tail.clear();
    }

    /// Subscribes from `since`: a replay when every later event is still kept, otherwise a snapshot (PRT-020).
    pub fn subscribe(&self, since: Option<&Position>) -> Subscription<E, S> {
        let inner = self.lock();
        let live = self.sender.subscribe();
        let replay = since.and_then(|position| {
            if *position.epoch != *inner.epoch || position.seq > inner.last_seq {
                return None;
            }
            let oldest_kept = inner
                .tail
                .front()
                .map_or(inner.last_seq + 1, |first| first.seq);
            (position.seq + 1 >= oldest_kept).then(|| {
                inner
                    .tail
                    .iter()
                    .filter(|event| event.seq > position.seq)
                    .map(|event| (**event).clone())
                    .collect()
            })
        });
        let start = match replay {
            Some(events) => Start::Replay(events),
            None => Start::Snapshot {
                epoch: Arc::clone(&inner.epoch),
                seq: inner.last_seq,
                state: inner.state.clone(),
            },
        };
        Subscription { start, live }
    }

    /// Reads the current state.
    pub fn with_state<R>(&self, read: impl FnOnce(&S) -> R) -> R {
        read(&self.lock().state)
    }

    /// Current position.
    #[must_use]
    pub fn position(&self) -> Position {
        let inner = self.lock();
        Position {
            epoch: inner.epoch.to_string(),
            seq: inner.last_seq,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::trivially_copy_pass_by_ref)] // Reducers take `fn(&mut S, &E)`.
    fn sum(state: &mut i64, event: &i64) {
        *state += event;
    }

    fn log(capacity: usize) -> EventLog<i64, i64> {
        EventLog::new("e1", 0, sum, capacity, 16)
    }

    fn at(epoch: &str, seq: u64) -> Position {
        Position {
            epoch: epoch.into(),
            seq,
        }
    }

    // spec: DMN-040
    #[test]
    fn events_are_numbered_and_folded_into_the_state() {
        let log = log(10);
        assert_eq!(log.push(5), 1);
        assert_eq!(log.push(-2), 2);
        assert_eq!(log.with_state(|state| *state), 3);
        assert_eq!(log.position(), at("e1", 2));
    }

    // spec: PRT-020, DMN-041
    #[test]
    fn a_known_position_inside_the_tail_replays_exactly_the_missed_events() {
        let log = log(10);
        for value in 1..=5 {
            log.push(value);
        }

        let Start::Replay(events) = log.subscribe(Some(&at("e1", 3))).start else {
            panic!("expected a replay");
        };

        let seqs: Vec<u64> = events.iter().map(|event| event.seq).collect();
        assert_eq!(seqs, [4, 5]);
        assert_eq!(events[0].event, 4);
    }

    // spec: PRT-020
    #[test]
    fn an_up_to_date_position_replays_nothing() {
        let log = log(10);
        log.push(1);
        assert_eq!(
            log.subscribe(Some(&at("e1", 1))).start,
            Start::Replay(Vec::new())
        );
    }

    // spec: PRT-020, DMN-041
    #[test]
    fn an_evicted_position_gets_a_snapshot_of_the_current_state() {
        let log = log(2);
        for value in 1..=5 {
            log.push(value);
        }

        let start = log.subscribe(Some(&at("e1", 1))).start;

        assert_eq!(
            start,
            Start::Snapshot {
                epoch: "e1".into(),
                seq: 5,
                state: 15
            }
        );
    }

    // spec: PRT-020
    #[test]
    fn no_position_another_epoch_or_a_future_seq_get_a_snapshot() {
        let log = log(10);
        log.push(7);
        for since in [None, Some(at("e0", 1)), Some(at("e1", 9))] {
            assert!(matches!(
                log.subscribe(since.as_ref()).start,
                Start::Snapshot {
                    seq: 1,
                    state: 7,
                    ..
                }
            ));
        }
    }

    // spec: PRT-021, PRT-022
    #[tokio::test]
    async fn live_events_continue_right_after_the_snapshot_without_gaps() {
        let log = log(10);
        log.push(1);
        let mut subscription = log.subscribe(None);
        log.push(2);
        log.push(3);

        let Start::Snapshot { seq, mut state, .. } = subscription.start else {
            panic!("expected a snapshot");
        };
        let mut seqs = Vec::new();
        for _ in 0..2 {
            let event = subscription.live.recv().await.unwrap();
            sum(&mut state, &event.event);
            seqs.push(event.seq);
        }

        assert_eq!(seq, 1);
        assert_eq!(seqs, [2, 3]);
        assert_eq!(state, log.with_state(|state| *state));
    }

    // spec: DMN-040, PRT-022
    #[test]
    fn a_new_epoch_restarts_numbering_and_keeps_the_state() {
        let log = log(10);
        log.push(4);
        log.new_epoch("e2");

        assert_eq!(log.push(1), 1);
        assert_eq!(log.position(), at("e2", 1));
        assert_eq!(log.with_state(|state| *state), 5);
        assert!(matches!(
            log.subscribe(Some(&at("e1", 1))).start,
            Start::Snapshot { .. }
        ));
    }

    // spec: PRT-023
    #[tokio::test]
    async fn a_subscriber_that_falls_too_far_behind_is_told_it_lagged() {
        let log: EventLog<i64, i64> = EventLog::new("e1", 0, sum, 10, 2);
        let mut subscription = log.subscribe(None);
        for value in 0..5 {
            log.push(value);
        }

        assert!(matches!(
            subscription.live.recv().await,
            Err(broadcast::error::RecvError::Lagged(_))
        ));
    }
}
