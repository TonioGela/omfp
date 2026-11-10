//! The omfp app (04-ui.md): Dioxus components over pure view-model functions.
//!
//! The UI never talks to the network. Its root, [`App`], reads one reactive [`ViewInput`] (index DTOs, one
//! [`omfp_core::ThreadState`] per open thread, connection status, intent outcomes) and emits [`Intent`]s through an
//! event handler; the host maps intents to protocol calls and feeds results back through the input. Everything only the
//! window cares about (selection, drafts, expanded cards, list grouping, panel placement, …) lives in [`UiState`]
//! (UI-010).
//!
//! The crate compiles natively without a renderer feature so its components are tested with `dioxus-ssr`.

mod components;
pub mod input;
pub mod intent;
pub mod local;
pub mod panels;
mod platform;
pub mod render;
mod views;
pub mod vm;

#[cfg(test)]
mod ssr_tests;
#[cfg(test)]
mod test_support;

pub use input::{
    ClosedInteraction, ConnectionView, OutcomeEntry, ThreadDetails, ThreadView, ViewInput,
};
pub use intent::{Intent, IntentOutcome, NewThread, QueueOp, WorkspaceSource};
pub use local::UiState;
pub use views::app::App;
