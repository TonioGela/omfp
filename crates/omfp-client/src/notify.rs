//! The model shared between the driver and the handles, with change notification for coalesced rendering (UI-012).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use crate::model::Model;

/// The model and the tasks waiting for it to change.
#[derive(Debug, Default)]
pub(crate) struct Shared {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    model: Model,
    waiting: Vec<Waker>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn read<R>(&self, read: impl FnOnce(&Model) -> R) -> R {
        read(&self.lock().model)
    }

    /// Changes the model; wakes every [`Changes`] waiter when its revision moved.
    pub(crate) fn update<R>(&self, update: impl FnOnce(&mut Model) -> R) -> R {
        let mut state = self.lock();
        let before = state.model.revision();
        let result = update(&mut state.model);
        if state.model.revision() != before {
            for waker in state.waiting.drain(..) {
                waker.wake();
            }
        }
        result
    }

    /// Drains the dirty set without counting as a change.
    pub(crate) fn take_dirty(&self) -> crate::model::Dirty {
        self.lock().model.take_dirty()
    }

    fn poll_changed(&self, seen: &mut u64, cx: &Context<'_>) -> Poll<()> {
        let mut state = self.lock();
        let revision = state.model.revision();
        if revision == *seen {
            if !state
                .waiting
                .iter()
                .any(|waker| waker.will_wake(cx.waker()))
            {
                state.waiting.push(cx.waker().clone());
            }
            Poll::Pending
        } else {
            *seen = revision;
            Poll::Ready(())
        }
    }
}

/// Wakes the host when the model changed; any number of changes in between wake it once, so a host awaiting
/// [`Changes::next`] before each animation frame renders at most once per frame (UI-012).
#[derive(Debug)]
pub struct Changes {
    shared: Arc<Shared>,
    seen: u64,
}

impl Changes {
    pub(crate) fn new(shared: Arc<Shared>) -> Self {
        let seen = shared.read(Model::revision);
        Self { shared, seen }
    }

    /// Resolves once the model changed since the previous call (or since this `Changes` was created).
    pub async fn next(&mut self) {
        std::future::poll_fn(|cx| self.shared.poll_changed(&mut self.seen, cx)).await;
    }
}
