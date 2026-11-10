//! Screens: the shell (sidebar, thread, panels, dialogs). Views read the shared [`Ctx`]; leaf components take props.

pub mod app;
mod composer;
mod header;
mod interactions;
mod new_thread;
mod panels;
mod sidebar;
mod thread;
mod transcript;

use std::collections::BTreeMap;

use dioxus::prelude::*;

use crate::input::ViewInput;
use crate::intent::Intent;
use crate::local::UiState;
use crate::vm::virtual_list::ListMetrics;

/// What every view shares: the input, the window state, transcript layout per thread, and the intent dispatcher.
#[derive(Clone, Copy)]
pub(crate) struct Ctx {
    pub input: ReadSignal<ViewInput>,
    pub ui: Signal<UiState>,
    pub metrics: Signal<BTreeMap<String, ListMetrics>>,
    pub dispatch: EventHandler<Intent>,
}

impl Ctx {
    pub fn get() -> Self {
        use_context::<Self>()
    }

    /// Sends `intent` to the host.
    pub fn send(&self, intent: Intent) {
        self.dispatch.call(intent);
    }

    /// Selects a thread and asks the host to open it.
    pub fn select_thread(mut self, thread_id: &str) {
        let updated_at = self
            .input
            .peek()
            .threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .map(|thread| thread.updated_at);
        let intent = self.ui.write().select(thread_id, updated_at);
        self.send(intent);
    }
}
