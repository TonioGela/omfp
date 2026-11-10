//! Data in: everything the UI renders that comes from the daemon, plus the connection status. The host keeps one
//! [`ViewInput`] in a signal and updates it in place; the UI only reads it.

use std::collections::BTreeMap;

use omfp_core::wire::{ExtensionUiRequest, ModelInfo, SessionState, ThinkingLevel};
use omfp_core::{Resolution, ThreadState};
use omfp_protocol::{GitStatus, Repository, ThreadSummary, Workspace};

use crate::intent::IntentOutcome;

/// Everything the root component renders.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewInput {
    pub connection: ConnectionView,
    /// The `index` topic: registered repositories, workspaces and threads, in index order.
    pub repositories: Vec<Repository>,
    pub workspaces: Vec<Workspace>,
    pub threads: Vec<ThreadSummary>,
    /// Last known git status per workspace id (`workspace/<id>` topic or `workspaces.status`).
    pub git: BTreeMap<String, GitStatus>,
    /// Subscribed threads by thread id.
    pub open: BTreeMap<String, ThreadView>,
    /// Results of dispatched intents, oldest first, each with a sequence number that increases by at least one per
    /// outcome. The UI applies every entry with a sequence number above the last one it applied; the host may drop
    /// entries the UI has seen.
    pub outcomes: Vec<OutcomeEntry>,
}

/// Status of the connection to the daemon (UI-003).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ConnectionView {
    /// First connection attempt in progress.
    #[default]
    Connecting,
    Connected,
    /// The connection was lost; attempt `attempt` starts in `retry_in_ms`.
    Reconnecting {
        attempt: u32,
        retry_in_ms: u64,
    },
    /// Retrying stopped.
    Failed {
        message: String,
    },
    /// The daemon rejected the token (close code 4401); the user must enter a new one.
    Unauthenticated,
}

/// One subscribed thread.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreadView {
    /// Built only by the `omfp-core` reducer (UI-010).
    pub state: ThreadState,
    /// Answers of `thread.state`, `thread.models` and `thread.thinking_levels`.
    pub details: ThreadDetails,
    /// Requests that left `state.interactions` through live events, oldest first (UI-072).
    pub closed: Vec<ClosedInteraction>,
}

/// Thread data fetched by calls rather than kept by the reducer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreadDetails {
    /// `thread.state`: model, thinking level, context usage.
    pub session: Option<SessionState>,
    /// `thread.models`.
    pub models: Vec<ModelInfo>,
    /// `thread.thinking_levels`.
    pub thinking_levels: Vec<ThinkingLevel>,
}

/// An interactive request that was closed while the UI watched the thread.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedInteraction {
    pub request: ExtensionUiRequest,
    /// Who answered it; `None` when omp closed it (timeout, abort) or the process exited.
    pub resolution: Option<Resolution>,
}

/// One [`IntentOutcome`] with its sequence number.
#[derive(Debug, Clone, PartialEq)]
pub struct OutcomeEntry {
    pub seq: u64,
    pub outcome: IntentOutcome,
}
