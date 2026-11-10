//! Pure client state kept from the daemon's frames: connection status, the `index` topic, per-thread
//! `omfp_core::ThreadState` and workspace git status, plus the position `{epoch, seq}` of every subscribed topic.
//!
//! Thread state comes only from the shared reducer (UI-010): snapshots are the daemon's reducer output, events are
//! applied with [`ThreadState::apply`] (PRT-021). Every change bumps [`Model::revision`] and lands in a [`Dirty`] set
//! the host drains once per frame (UI-012).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use omfp_core::wire::ExtensionUiRequest;
use omfp_core::{Resolution, ThreadEvent, ThreadState};
use omfp_protocol::{
    ErrorBody, ErrorCode, EventFrame, HelloResult, IndexEvent, IndexSnapshot, Position, RepoStatus,
    Repository, SubscribeParams, SubscribeResult, ThreadSummary, Topic, TopicEvent, Workspace,
    WorkspaceEvent,
};

/// Closed interactions kept per thread (newest last).
pub const CLOSED_KEPT: usize = 32;

/// State of the connection to the daemon.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// First connection attempt.
    #[default]
    Connecting,
    /// `system.hello` succeeded.
    Connected(HelloResult),
    /// The connection failed or was lost; attempt `attempt` starts after `retry_in` (UI-003).
    Reconnecting {
        attempt: u32,
        retry_in: Duration,
        error: String,
    },
    /// The daemon refused the token (close 4401); the client does not retry.
    Unauthenticated,
    /// The daemon does not speak this client's protocol version (PRT-004); the client does not retry.
    VersionMismatch {
        supported: Vec<u32>,
        message: String,
    },
    /// The client was closed by the host.
    Closed,
}

/// The `index` topic: repositories, workspaces and thread summaries keyed by id (ids are ULIDs, so key order is
/// creation order).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Index {
    pub repositories: BTreeMap<String, Repository>,
    pub workspaces: BTreeMap<String, Workspace>,
    pub threads: BTreeMap<String, ThreadSummary>,
}

impl From<IndexSnapshot> for Index {
    fn from(snapshot: IndexSnapshot) -> Self {
        Self {
            repositories: snapshot
                .repositories
                .into_iter()
                .map(|repository| (repository.id.clone(), repository))
                .collect(),
            workspaces: snapshot
                .workspaces
                .into_iter()
                .map(|workspace| (workspace.id.clone(), workspace))
                .collect(),
            threads: snapshot
                .threads
                .into_iter()
                .map(|thread| (thread.id.clone(), thread))
                .collect(),
        }
    }
}

impl Index {
    fn apply(&mut self, event: &IndexEvent) {
        match event {
            IndexEvent::RepositoryUpserted { repository } => {
                self.repositories
                    .insert(repository.id.clone(), repository.clone());
            }
            IndexEvent::RepositoryRemoved { repository_id } => {
                self.repositories.remove(repository_id);
            }
            IndexEvent::WorkspaceUpserted { workspace } => {
                self.workspaces
                    .insert(workspace.id.clone(), workspace.clone());
            }
            IndexEvent::WorkspaceRemoved { workspace_id } => {
                self.workspaces.remove(workspace_id);
            }
            IndexEvent::ThreadUpserted { thread } => {
                self.threads.insert(thread.id.clone(), thread.clone());
            }
            IndexEvent::ThreadRemoved { thread_id } => {
                self.threads.remove(thread_id);
            }
        }
    }
}

/// An interactive request that is no longer pending.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedInteraction {
    pub request: ExtensionUiRequest,
    /// Who answered (UI-072); `None` when omp cancelled it or the process went away.
    pub resolution: Option<Resolution>,
}

/// One subscribed thread.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreadView {
    /// The reducer's state (UI-010).
    pub state: ThreadState,
    /// Requests that left `state.interactions` while live events were applied, newest last, at most
    /// [`CLOSED_KEPT`]. A snapshot carries no such history and starts the list afresh.
    pub closed: Vec<ClosedInteraction>,
}

impl ThreadView {
    #[must_use]
    pub fn new(state: ThreadState) -> Self {
        Self {
            state,
            closed: Vec::new(),
        }
    }

    /// Applies one event with the shared reducer and records the interactions it closed.
    pub fn apply(&mut self, event: &ThreadEvent) {
        let pending = if self.state.interactions.is_empty() {
            Vec::new()
        } else {
            self.state.interactions.clone()
        };
        self.state.apply(event);
        let resolution = match event {
            ThreadEvent::InteractionResolved { resolution, .. } => Some(resolution),
            _ => None,
        };
        for request in pending {
            if !self.state.interactions.contains(&request) {
                self.closed.push(ClosedInteraction {
                    request,
                    resolution: resolution.cloned(),
                });
            }
        }
        if self.closed.len() > CLOSED_KEPT {
            self.closed.drain(..self.closed.len() - CLOSED_KEPT);
        }
    }
}

/// How a subscription started (PRT-020).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartMode {
    /// The daemon replayed the missed events after the known position.
    Replay,
    /// The daemon sent the full state.
    Snapshot,
}

/// Where a subscribed topic stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopicSync {
    /// A subscribe request is due or in flight; events are ignored until it is answered.
    Subscribing,
    /// Events apply in order.
    Live(StartMode),
    /// The daemon refused the subscription (retried on the next connection).
    Failed(ErrorBody),
}

/// A topic the client wants, with its last applied position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicState {
    pub topic: Topic,
    /// Last applied `{epoch, seq}`; sent as `since` when resubscribing (UI-003, PRT-020).
    pub position: Option<Position>,
    pub sync: TopicSync,
}

/// What changed since the host last drained it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dirty {
    pub connection: bool,
    /// Subscriptions were added, removed or changed state.
    pub topics: bool,
    pub index: bool,
    /// Thread ids whose [`ThreadView`] changed or went away.
    pub threads: BTreeSet<String>,
    /// Workspace ids whose git status changed or went away.
    pub workspaces: BTreeSet<String>,
}

impl Dirty {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

/// Result of [`Model::apply_event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// The event continued the stream and was applied.
    Applied,
    /// Not for a live subscription, or already applied (duplicate).
    Ignored,
    /// The stream skipped events (PRT-022): the topic must be resubscribed; its position was dropped, so the
    /// daemon answers with a snapshot.
    Gap,
}

/// A subscribe result the model cannot use.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SubscribeError {
    #[error("{0} is not subscribed")]
    NotTracked(Topic),
    /// The replay does not continue the known position: resubscribe without `since`.
    #[error("replay of {topic} from {epoch}/{from_seq} does not continue the known position")]
    ReplayMismatch {
        topic: Topic,
        epoch: String,
        from_seq: u64,
    },
    /// The snapshot does not decode as the topic's state; the topic is marked failed.
    #[error("invalid snapshot for {topic}: {message}")]
    Snapshot { topic: Topic, message: String },
}

/// The client's state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    connection: ConnectionStatus,
    index: Index,
    threads: BTreeMap<String, ThreadView>,
    workspaces: BTreeMap<String, Vec<RepoStatus>>,
    topics: Vec<TopicState>,
    dirty: Dirty,
    revision: u64,
}

impl Model {
    #[must_use]
    pub fn connection(&self) -> &ConnectionStatus {
        &self.connection
    }

    /// The `index` topic's state (empty until its snapshot arrives).
    #[must_use]
    pub fn index(&self) -> &Index {
        &self.index
    }

    /// Subscribed threads that received their snapshot.
    #[must_use]
    pub fn threads(&self) -> &BTreeMap<String, ThreadView> {
        &self.threads
    }

    #[must_use]
    pub fn thread(&self, id: &str) -> Option<&ThreadView> {
        self.threads.get(id)
    }

    /// Git status of each repository of subscribed workspaces.
    #[must_use]
    pub fn workspaces(&self) -> &BTreeMap<String, Vec<RepoStatus>> {
        &self.workspaces
    }

    /// Subscribed topics in subscription order.
    #[must_use]
    pub fn topics(&self) -> &[TopicState] {
        &self.topics
    }

    #[must_use]
    pub fn topic(&self, topic: &Topic) -> Option<&TopicState> {
        self.topics.iter().find(|state| state.topic == *topic)
    }

    /// Increases with every visible change.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// What changed since the previous call.
    pub fn take_dirty(&mut self) -> Dirty {
        std::mem::take(&mut self.dirty)
    }

    pub fn set_connection(&mut self, status: ConnectionStatus) {
        if self.connection != status {
            self.connection = status;
            self.dirty.connection = true;
            self.revision += 1;
        }
    }

    fn topics_changed(&mut self) {
        self.dirty.topics = true;
        self.revision += 1;
    }

    fn entry(&mut self, topic: &Topic) -> Option<&mut TopicState> {
        self.topics.iter_mut().find(|state| state.topic == *topic)
    }

    /// Adds `topic` to the wanted subscriptions. Returns whether a subscribe request is due: the topic is new, or
    /// its previous subscription failed.
    pub fn track(&mut self, topic: Topic) -> bool {
        if let Some(state) = self.entry(&topic) {
            return matches!(state.sync, TopicSync::Failed(_));
        }
        self.topics.push(TopicState {
            topic,
            position: None,
            sync: TopicSync::Subscribing,
        });
        self.topics_changed();
        true
    }

    /// Drops `topic` and its data. Returns whether it was subscribed.
    pub fn untrack(&mut self, topic: &Topic) -> bool {
        let Some(at) = self.topics.iter().position(|state| state.topic == *topic) else {
            return false;
        };
        self.topics.remove(at);
        match topic {
            Topic::Index => {
                self.index = Index::default();
                self.dirty.index = true;
            }
            Topic::Thread(id) => {
                self.threads.remove(id);
                self.dirty.threads.insert(id.clone());
            }
            Topic::Workspace(id) => {
                self.workspaces.remove(id);
                self.dirty.workspaces.insert(id.clone());
            }
            Topic::Terminal(_) => {}
        }
        self.topics_changed();
        true
    }

    /// The params to (re)subscribe `topic` from its last position; marks it subscribing. `None` when not tracked.
    pub fn subscribe_params(&mut self, topic: &Topic) -> Option<SubscribeParams> {
        let state = self.entry(topic)?;
        let params = SubscribeParams {
            topic: topic.clone(),
            since: state.position.clone(),
        };
        if state.sync != TopicSync::Subscribing {
            state.sync = TopicSync::Subscribing;
            self.topics_changed();
        }
        Some(params)
    }

    /// `resync_required` (PRT-023), or a stream the client cannot continue: the next subscription asks for a
    /// snapshot. The data stays until the snapshot replaces it. Returns whether the topic is tracked.
    pub fn resync(&mut self, topic: &Topic) -> bool {
        let Some(state) = self.entry(topic) else {
            return false;
        };
        state.position = None;
        state.sync = TopicSync::Subscribing;
        self.topics_changed();
        true
    }

    /// The connection dropped: live topics wait for their resubscription; positions are kept for `since`.
    pub fn connection_lost(&mut self) {
        let mut changed = false;
        for state in &mut self.topics {
            if matches!(state.sync, TopicSync::Live(_)) {
                state.sync = TopicSync::Subscribing;
                changed = true;
            }
        }
        if changed {
            self.topics_changed();
        }
    }

    /// The daemon refused to subscribe `topic`.
    pub fn fail_topic(&mut self, topic: &Topic, error: ErrorBody) {
        if let Some(state) = self.entry(topic) {
            state.sync = TopicSync::Failed(error);
            self.topics_changed();
        }
    }

    /// Applies the answer to a subscribe request (PRT-020): a replay continues from the known position, a snapshot
    /// replaces the topic's state and position.
    pub fn apply_subscribed(
        &mut self,
        topic: &Topic,
        result: SubscribeResult,
    ) -> Result<StartMode, SubscribeError> {
        let Some(state) = self.entry(topic) else {
            return Err(SubscribeError::NotTracked(topic.clone()));
        };
        match result {
            SubscribeResult::Replay { epoch, from_seq } => {
                let continues = state.position.as_ref().is_some_and(|position| {
                    position.epoch == epoch && from_seq == position.seq + 1
                });
                if continues {
                    state.sync = TopicSync::Live(StartMode::Replay);
                    self.topics_changed();
                    Ok(StartMode::Replay)
                } else {
                    self.resync(topic);
                    Err(SubscribeError::ReplayMismatch {
                        topic: topic.clone(),
                        epoch,
                        from_seq,
                    })
                }
            }
            SubscribeResult::Snapshot { epoch, seq, state } => {
                if let Err(message) = self.replace(topic, state) {
                    let error = ErrorBody::new(ErrorCode::Internal, message.clone());
                    self.fail_topic(topic, error);
                    return Err(SubscribeError::Snapshot {
                        topic: topic.clone(),
                        message,
                    });
                }
                if let Some(entry) = self.entry(topic) {
                    entry.position = Some(Position { epoch, seq });
                    entry.sync = TopicSync::Live(StartMode::Snapshot);
                }
                self.topics_changed();
                Ok(StartMode::Snapshot)
            }
        }
    }

    /// Replaces a topic's data with a snapshot's `state`.
    fn replace(&mut self, topic: &Topic, state: serde_json::Value) -> Result<(), String> {
        let invalid = |error: serde_json::Error| error.to_string();
        match topic {
            Topic::Index => {
                let snapshot: IndexSnapshot = serde_json::from_value(state).map_err(invalid)?;
                self.index = Index::from(snapshot);
                self.dirty.index = true;
            }
            Topic::Thread(id) => {
                let state: ThreadState = serde_json::from_value(state).map_err(invalid)?;
                self.threads.insert(id.clone(), ThreadView::new(state));
                self.dirty.threads.insert(id.clone());
            }
            Topic::Workspace(id) => {
                let repos: Vec<RepoStatus> = serde_json::from_value(state).map_err(invalid)?;
                self.workspaces.insert(id.clone(), repos);
                self.dirty.workspaces.insert(id.clone());
            }
            Topic::Terminal(_) => return Err("terminal topics have no snapshot state".into()),
        }
        Ok(())
    }

    /// Applies one event frame if it continues its topic's stream (PRT-022): the next `seq` of the same epoch, or
    /// `seq` 1 of a new epoch (a new omp process; the daemon's state carries over). Duplicates are ignored; a gap
    /// drops the position and asks for a resubscription.
    pub fn apply_event(&mut self, frame: &EventFrame) -> Applied {
        let Some(state) = self.entry(&frame.topic) else {
            return Applied::Ignored;
        };
        if !matches!(state.sync, TopicSync::Live(_)) {
            return Applied::Ignored;
        }
        let continues = match &state.position {
            Some(position) if position.epoch == frame.epoch => {
                if frame.seq <= position.seq {
                    return Applied::Ignored;
                }
                frame.seq == position.seq + 1
            }
            Some(_) => frame.seq == 1,
            None => false,
        };
        if !continues {
            self.resync(&frame.topic);
            return Applied::Gap;
        }
        state.position = Some(Position {
            epoch: frame.epoch.clone(),
            seq: frame.seq,
        });
        match (&frame.topic, &frame.event) {
            (Topic::Index, TopicEvent::Index(event)) => {
                self.index.apply(event);
                self.dirty.index = true;
            }
            (Topic::Thread(id), TopicEvent::Thread(event)) => {
                self.threads.entry(id.clone()).or_default().apply(event);
                self.dirty.threads.insert(id.clone());
            }
            (Topic::Workspace(id), TopicEvent::Workspace(WorkspaceEvent::Status { repos })) => {
                self.workspaces.insert(id.clone(), repos.clone());
                self.dirty.workspaces.insert(id.clone());
            }
            // Decoding ties the event kind to the topic.
            _ => {}
        }
        self.revision += 1;
        Applied::Applied
    }
}
