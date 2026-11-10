//! The transport suite (TST-030, UI-001): the same scenarios run against a real daemon with [`NativeTransport`]
//! (`transport_native.rs`, L2) and [`WebTransport`] in headless Chromium (`transport_web.rs`, `just test-wasm`).
//!
//! Scenarios are runtime-agnostic: the client's driver runs beside the scenario body in one future, and connections
//! go through a [`Switch`] that lets a scenario drop the live connection or hold reconnect attempts.
//!
//! [`NativeTransport`]: omfp_client::NativeTransport
//! [`WebTransport`]: omfp_client::WebTransport

#![allow(dead_code)] // Each test binary uses a different subset.

use std::pin::pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Poll, Waker};
use std::time::Duration;

use futures::channel::oneshot;
use futures::future::{Either, join, select};
use omfp_client::model::{ConnectionStatus, StartMode, TopicSync};
use omfp_client::{
    Backoff, CallError, Client, ClientConfig, Closed, Connector, Incoming, Model, Timer, Transport,
    TransportError, connect,
};
use omfp_core::ProcessState;
use omfp_protocol::method::ThreadsCreate;
use omfp_protocol::{Call, CreateThreadParams, Empty, PROTOCOL_VERSION, ThreadRef, Topic};

/// How long a scenario waits for a condition before failing.
pub const WAIT: Duration = Duration::from_secs(30);

/// A daemon to test against and the transport under test.
pub trait Harness {
    type Transport: Transport;
    type Timer: Timer + Clone;

    /// `ws://…/api/ws` of the daemon.
    fn url(&self) -> String;
    fn token(&self) -> String;
    fn timer(&self) -> Self::Timer;
    fn open(url: String) -> impl Future<Output = Result<Self::Transport, TransportError>>;
}

fn config(token: String, client: &str) -> ClientConfig {
    ClientConfig {
        backoff: Backoff {
            initial: Duration::from_millis(20),
            max: Duration::from_millis(200),
        },
        ..ClientConfig::new(token, client)
    }
}

/// Lets a scenario drop the live connection and hold reconnect attempts.
#[derive(Debug, Clone, Default)]
pub struct Switch {
    state: Arc<Mutex<SwitchState>>,
}

#[derive(Debug, Default)]
struct SwitchState {
    attempts: usize,
    held: bool,
    waiting: Vec<Waker>,
    drop_live: Option<oneshot::Sender<()>>,
}

impl Switch {
    fn lock(&self) -> MutexGuard<'_, SwitchState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Connection attempts so far.
    pub fn attempts(&self) -> usize {
        self.lock().attempts
    }

    /// Later attempts wait until [`Switch::release`].
    pub fn hold(&self) {
        self.lock().held = true;
    }

    pub fn release(&self) {
        let mut state = self.lock();
        state.held = false;
        for waker in state.waiting.drain(..) {
            waker.wake();
        }
    }

    /// Closes the live connection from the client side, as a network failure would.
    pub fn drop_connection(&self) {
        if let Some(drop_live) = self.lock().drop_live.take() {
            let _ = drop_live.send(());
        }
    }

    async fn released(&self) {
        std::future::poll_fn(|cx| {
            let mut state = self.lock();
            if state.held {
                state.waiting.push(cx.waker().clone());
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
    }

    /// A connector for `H`'s transport to `url` through this switch.
    pub fn connector<H: Harness>(&self, url: String) -> impl Connector + use<H> {
        let switch = self.clone();
        move || {
            let switch = switch.clone();
            let url = url.clone();
            async move {
                switch.released().await;
                switch.lock().attempts += 1;
                let inner = H::open(url).await?;
                let (drop_live, dropped) = oneshot::channel();
                switch.lock().drop_live = Some(drop_live);
                Ok(Droppable {
                    inner,
                    dropped,
                    gone: false,
                })
            }
        }
    }
}

/// A transport the [`Switch`] can drop.
pub struct Droppable<T> {
    inner: T,
    dropped: oneshot::Receiver<()>,
    gone: bool,
}

impl<T: Transport> Transport for Droppable<T> {
    async fn send(&mut self, text: String) -> Result<(), TransportError> {
        self.inner.send(text).await
    }

    async fn recv(&mut self) -> Incoming {
        if !self.gone {
            {
                let received = pin!(self.inner.recv());
                if let Either::Left((incoming, _)) = select(received, &mut self.dropped).await {
                    return incoming;
                }
            }
            self.gone = true;
            self.inner.close().await;
        }
        Incoming::Closed(Closed {
            code: None,
            reason: "dropped by the test".into(),
        })
    }

    async fn close(&mut self) {
        self.inner.close().await;
    }
}

/// Runs `body` while `driver` serves the client(s); `body`'s output once it is done.
pub async fn with_driver<F: Future>(driver: impl Future, body: F) -> F::Output {
    match select(pin!(body), pin!(driver)).await {
        Either::Left((output, _)) => output,
        Either::Right((_, body)) => body.await,
    }
}

/// Waits until `check` holds for the client's model.
pub async fn until<T: Timer>(
    client: &Client,
    timer: &T,
    what: &str,
    check: impl Fn(&Model) -> bool,
) {
    let mut changes = client.changes();
    let holds = async {
        while !client.read(&check) {
            changes.next().await;
        }
    };
    if let Either::Right(_) = select(pin!(holds), pin!(timer.sleep(WAIT))).await {
        panic!(
            "timed out waiting for {what}: {:#?}",
            client.read(Clone::clone)
        );
    }
}

fn connected(model: &Model) -> bool {
    matches!(model.connection(), ConnectionStatus::Connected(_))
}

fn live(model: &Model, topic: &Topic, mode: StartMode) -> bool {
    model
        .topic(topic)
        .is_some_and(|state| state.sync == TopicSync::Live(mode))
}

/// Creates a thread with `client`.
pub async fn create_thread(client: &Client, title: &str) -> String {
    let params = CreateThreadParams {
        title: title.to_owned(),
        workspace: omfp_protocol::ThreadWorkspace::default(),
        tags: Vec::new(),
        model: None,
        thinking_level: None,
        approval_mode: None,
        prompt: None,
    };
    client
        .request::<ThreadsCreate>(params)
        .await
        .expect("threads.create")
        .id
}

/// `system.hello` with the token connects and reports the daemon's versions (PRT-001, PRT-005).
pub async fn hello<H: Harness>(harness: &H) {
    let timer = harness.timer();
    let switch = Switch::default();
    let (client, driver) = connect(
        switch.connector::<H>(harness.url()),
        timer.clone(),
        config(harness.token(), "suite/hello"),
    );
    with_driver(driver, async {
        until(&client, &timer, "hello", connected).await;
        let ConnectionStatus::Connected(hello) = client.read(|model| model.connection().clone())
        else {
            unreachable!()
        };
        assert_eq!(hello.protocol, PROTOCOL_VERSION);
        assert_eq!(hello.omp, omfp_core::OMP_VERSION);
        client.close();
        until(&client, &timer, "closed", |model| {
            model.connection() == &ConnectionStatus::Closed
        })
        .await;
    })
    .await;
    assert_eq!(switch.attempts(), 1);
}

/// A wrong token ends as unauthenticated, without a retry (close 4401).
pub async fn auth_failure<H: Harness>(harness: &H) {
    let timer = harness.timer();
    let switch = Switch::default();
    let (client, driver) = connect(
        switch.connector::<H>(harness.url()),
        timer.clone(),
        config("not-the-token".into(), "suite/auth"),
    );
    let pending = client.call(Call::ReposList(Empty {}));

    // The driver ends by itself.
    if let Either::Right(_) = select(pin!(driver), pin!(timer.sleep(WAIT))).await {
        panic!("the driver kept running: {:#?}", client.read(Clone::clone));
    }

    assert_eq!(
        client.read(|model| model.connection().clone()),
        ConnectionStatus::Unauthenticated
    );
    assert_eq!(switch.attempts(), 1);
    assert_eq!(pending.await, Err(CallError::Closed));
}

/// A client connecting straight to the daemon (no [`Switch`]).
pub fn connect_plain<H: Harness>(
    harness: &H,
    name: &str,
) -> (Client, impl Future<Output = ()> + use<H>) {
    let url = harness.url();
    connect(
        move || H::open(url.clone()),
        harness.timer(),
        config(harness.token(), name),
    )
}

/// Waits until both clients applied `topic` up to the same position.
async fn same_position<T: Timer>(timer: &T, a: &Client, b: &Client, topic: &Topic) {
    let position = |model: &Model| model.topic(topic).and_then(|state| state.position.clone());
    let mut a_changes = a.changes();
    let mut b_changes = b.changes();
    let converged = async {
        while a.read(position) != b.read(position) {
            select(pin!(a_changes.next()), pin!(b_changes.next())).await;
        }
    };
    if let Either::Right(_) = select(pin!(converged), pin!(timer.sleep(WAIT))).await {
        panic!("{topic}: {:?} != {:?}", a.read(position), b.read(position));
    }
}

/// Subscribing without a position gets snapshots; the thread then follows its events, and a second client's
/// snapshot equals the state followed from events at the same position (PRT-020, PRT-021).
pub async fn subscribe_snapshot<H: Harness>(harness: &H) {
    let timer = harness.timer();
    let (client, driver) = connect_plain(harness, "suite/snapshot");
    let (observer, observer_driver) = connect_plain(harness, "suite/observer");
    with_driver(join(driver, observer_driver), async {
        let thread = create_thread(&client, "snapshot").await;
        let topic = Topic::Thread(thread.clone());
        client.subscribe(Topic::Index);
        client.subscribe(topic.clone());
        until(&client, &timer, "snapshots", |model| {
            live(model, &Topic::Index, StartMode::Snapshot)
                && live(model, &topic, StartMode::Snapshot)
        })
        .await;
        client.read(|model| {
            assert_eq!(model.index().threads[&thread].title, "snapshot");
            assert_eq!(
                model.thread(&thread).unwrap().state.process,
                ProcessState::Stopped
            );
        });

        // Starts the (fake) omp process: its events follow the snapshot. (The fake answers `get_state` with `{}`.)
        client
            .call(Call::ThreadState(ThreadRef {
                thread_id: thread.clone(),
            }))
            .await
            .expect("thread.state");
        until(&client, &timer, "a running thread", |model| {
            model.thread(&thread).is_some_and(|view| {
                view.state.process == ProcessState::Running && view.state.session.is_some()
            })
        })
        .await;

        observer.subscribe(topic.clone());
        until(&observer, &timer, "the observer's snapshot", |model| {
            live(model, &topic, StartMode::Snapshot)
        })
        .await;
        same_position(&timer, &client, &observer, &topic).await;
        assert_eq!(
            client.read(|model| model.thread(&thread).cloned().map(|view| view.state)),
            observer.read(|model| model.thread(&thread).cloned().map(|view| view.state))
        );
    })
    .await;
}

/// After a lost connection the client resubscribes from its position and the daemon replays what it missed
/// (UI-003, PRT-020).
pub async fn replay<H: Harness>(harness: &H) {
    let timer = harness.timer();
    let switch = Switch::default();
    let (client, driver) = connect(
        switch.connector::<H>(harness.url()),
        timer.clone(),
        config(harness.token(), "suite/replay"),
    );
    let (other, other_driver) = connect_plain(harness, "suite/other");
    with_driver(join(driver, other_driver), async {
        client.subscribe(Topic::Index);
        until(&client, &timer, "the index snapshot", |model| {
            live(model, &Topic::Index, StartMode::Snapshot)
        })
        .await;

        switch.hold();
        switch.drop_connection();
        until(&client, &timer, "reconnecting", |model| {
            matches!(model.connection(), ConnectionStatus::Reconnecting { .. })
        })
        .await;
        let missed = create_thread(&other, "missed while offline").await;
        switch.release();

        until(&client, &timer, "the missed thread", |model| {
            model.index().threads.contains_key(&missed)
        })
        .await;
        client.read(|model| {
            assert!(connected(model));
            assert!(live(model, &Topic::Index, StartMode::Replay));
        });
        assert_eq!(switch.attempts(), 2);
    })
    .await;
}

/// A dropped connection is re-established by itself: hello again, subscriptions resumed, calls work (UI-003).
pub async fn reconnect<H: Harness>(harness: &H) {
    let timer = harness.timer();
    let switch = Switch::default();
    let (client, driver) = connect(
        switch.connector::<H>(harness.url()),
        timer.clone(),
        config(harness.token(), "suite/reconnect"),
    );
    with_driver(driver, async {
        client.subscribe(Topic::Index);
        until(&client, &timer, "the index snapshot", |model| {
            connected(model) && live(model, &Topic::Index, StartMode::Snapshot)
        })
        .await;

        switch.drop_connection();
        until(&client, &timer, "the second connection", |model| {
            switch.attempts() == 2
                && connected(model)
                && model
                    .topic(&Topic::Index)
                    .is_some_and(|state| matches!(state.sync, TopicSync::Live(_)))
        })
        .await;
        let created = create_thread(&client, "after reconnect").await;

        until(&client, &timer, "the new thread", |model| {
            model.index().threads.contains_key(&created)
        })
        .await;
    })
    .await;
}
