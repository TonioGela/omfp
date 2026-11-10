//! Domain model and pure reducer over omp events.
//!
//! This crate performs no I/O and compiles to `wasm32-unknown-unknown` (ARC-010). The daemon runs it; the UI runs its
//! TypeScript port (`ui/packages/core`), checked against this reducer's golden states (D-04).

mod ask;
mod event;
mod reducer;
mod state;
mod stream;

pub use ask::validate_ask_answers;
pub use event::{ProcessState, Resolution, SessionInfo, ThreadEvent};
pub use reducer::replay;
pub use state::{
    Notice, NoticeFrame, Presentation, QueueState, StreamingMessage, Subagent, ThreadState,
    ToolRun, ToolStatus, TranscriptEntry, Widget,
};

/// omp RPC wire types, reused as-is instead of being redefined (ARC-011).
pub use omp_rpc::wire;

/// The omp release whose RPC protocol this build speaks.
///
/// Must equal the `omp-rpc` git tag in `Cargo.toml`, the `oh-my-pi` flake input, and `omp --version` (ENV-010).
pub const OMP_VERSION: &str = "18.18.6";
