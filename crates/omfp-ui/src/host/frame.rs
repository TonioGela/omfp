//! Per-frame coalescing (UI-012): however many frames the daemon streams, the view input is updated at most once per
//! animation frame, with everything that changed since the previous update.

use omfp_client::model::{Dirty, Model};
use omfp_client::{Changes, Client};

/// Where model changes come from: the client, or a fake in tests.
pub trait ModelSource {
    /// Resolves once the model changed since the previous call; any number of changes resolve it once.
    fn changed(&mut self) -> impl Future<Output = ()>;
    /// What changed since the previous call.
    fn take_dirty(&self) -> Dirty;
    fn read<R>(&self, read: impl FnOnce(&Model) -> R) -> R;
}

/// [`ModelSource`] of a running client.
#[derive(Debug)]
pub struct ClientSource {
    client: Client,
    changes: Changes,
}

impl ClientSource {
    #[must_use]
    pub fn new(client: Client) -> Self {
        let changes = client.changes();
        Self { client, changes }
    }
}

impl ModelSource for ClientSource {
    fn changed(&mut self) -> impl Future<Output = ()> {
        self.changes.next()
    }

    fn take_dirty(&self) -> Dirty {
        self.client.take_dirty()
    }

    fn read<R>(&self, read: impl FnOnce(&Model) -> R) -> R {
        self.client.read(read)
    }
}

/// The display's frame clock: `requestAnimationFrame` in the browser.
pub trait Frames {
    /// Resolves at the next animation frame.
    fn next(&self) -> impl Future<Output = ()>;
}

/// Calls `render` with the model and what changed, at most once per frame and only when something changed. Runs
/// until the host drops it.
pub async fn render_loop<S: ModelSource, F: Frames>(
    mut source: S,
    frames: F,
    mut render: impl FnMut(&Model, &Dirty),
) {
    // Whatever the model holds before the first change notification (the loop may start late).
    let mut dirty = Some(everything(&source));
    loop {
        if dirty.is_none() {
            source.changed().await;
        }
        frames.next().await;
        let mut changed = source.take_dirty();
        if let Some(initial) = dirty.take() {
            changed = merge(initial, changed);
        }
        if !changed.is_empty() {
            source.read(|model| render(model, &changed));
        }
    }
}

/// A dirty set naming every part of the model.
fn everything(source: &impl ModelSource) -> Dirty {
    source.read(|model| Dirty {
        connection: true,
        topics: true,
        index: true,
        threads: model.threads().keys().cloned().collect(),
        workspaces: model.workspaces().keys().cloned().collect(),
    })
}

fn merge(mut into: Dirty, from: Dirty) -> Dirty {
    into.connection |= from.connection;
    into.topics |= from.topics;
    into.index |= from.index;
    into.threads.extend(from.threads);
    into.workspaces.extend(from.workspaces);
    into
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use futures::StreamExt;
    use futures::channel::mpsc;
    use futures::executor::LocalPool;
    use futures::task::LocalSpawnExt;
    use omfp_client::model::ConnectionStatus;
    use omfp_protocol::Topic;

    use super::*;

    /// A model changed by the test, with a revision like the client's.
    #[derive(Default)]
    struct Shared {
        model: RefCell<Model>,
        notified: Cell<u64>,
        wakers: RefCell<Vec<std::task::Waker>>,
    }

    struct FakeSource(Rc<Shared>);

    impl FakeSource {
        fn change(shared: &Shared, change: impl FnOnce(&mut Model)) {
            change(&mut shared.model.borrow_mut());
            for waker in shared.wakers.borrow_mut().drain(..) {
                waker.wake();
            }
        }
    }

    impl ModelSource for FakeSource {
        fn changed(&mut self) -> impl Future<Output = ()> {
            let shared = Rc::clone(&self.0);
            std::future::poll_fn(move |cx| {
                let revision = shared.model.borrow().revision();
                if revision == shared.notified.get() {
                    shared.wakers.borrow_mut().push(cx.waker().clone());
                    std::task::Poll::Pending
                } else {
                    shared.notified.set(revision);
                    std::task::Poll::Ready(())
                }
            })
        }

        fn take_dirty(&self) -> Dirty {
            self.0.model.borrow_mut().take_dirty()
        }

        fn read<R>(&self, read: impl FnOnce(&Model) -> R) -> R {
            read(&self.0.model.borrow())
        }
    }

    /// Frames the test releases one by one.
    struct FakeFrames(Rc<RefCell<mpsc::UnboundedReceiver<()>>>);

    impl Frames for FakeFrames {
        fn next(&self) -> impl Future<Output = ()> {
            let frames = Rc::clone(&self.0);
            std::future::poll_fn(move |cx| frames.borrow_mut().poll_next_unpin(cx).map(|_| ()))
        }
    }

    // spec: UI-012
    #[test]
    fn many_model_changes_render_once_per_frame() {
        let shared = Rc::new(Shared::default());
        let (frame_tx, frame_rx) = mpsc::unbounded();
        let renders: Rc<RefCell<Vec<Dirty>>> = Rc::default();
        let mut pool = LocalPool::new();
        {
            let source = FakeSource(Rc::clone(&shared));
            let frames = FakeFrames(Rc::new(RefCell::new(frame_rx)));
            let renders = Rc::clone(&renders);
            pool.spawner()
                .spawn_local(render_loop(source, frames, move |_, dirty| {
                    renders.borrow_mut().push(dirty.clone());
                }))
                .unwrap();
        }
        // The first frame shows what the model held when the loop started.
        pool.run_until_stalled();
        frame_tx.unbounded_send(()).unwrap();
        pool.run_until_stalled();
        assert_eq!(renders.borrow().len(), 1);

        // A burst of changes between two frames: nothing renders until the frame, then once with all of them.
        for attempt in 1..=50 {
            FakeSource::change(&shared, |model| {
                model.set_connection(ConnectionStatus::Reconnecting {
                    attempt,
                    retry_in: std::time::Duration::from_millis(500),
                    error: "lost".into(),
                });
            });
            FakeSource::change(&shared, |model| {
                model.track(Topic::Thread(format!("t{attempt}")));
            });
            pool.run_until_stalled();
        }
        assert_eq!(renders.borrow().len(), 1);
        frame_tx.unbounded_send(()).unwrap();
        pool.run_until_stalled();
        assert_eq!(renders.borrow().len(), 2);
        let last = renders.borrow()[1].clone();
        assert!(last.connection && last.topics);

        // Frames without changes render nothing.
        frame_tx.unbounded_send(()).unwrap();
        frame_tx.unbounded_send(()).unwrap();
        pool.run_until_stalled();
        assert_eq!(renders.borrow().len(), 2);
        assert!(matches!(
            shared.model.borrow().connection(),
            ConnectionStatus::Reconnecting { attempt: 50, .. }
        ));
    }
}
