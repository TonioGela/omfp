//! A scripted in-memory network and a manual timer for L1 session tests: the test plays the daemon, decides every
//! connection attempt, and fires sleeps by hand, so reconnect behavior is checked without real time (TST-003).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use futures::executor::LocalPool;
use futures::task::LocalSpawnExt;
use omfp_client::model::ConnectionStatus;
use omfp_client::{
    Client, ClientConfig, Closed, Connector, Incoming, Timer, Transport, TransportError, connect,
};
use omfp_protocol::Call;
use serde_json::{Value, json};

type Answer = oneshot::Sender<Result<FakeTransport, TransportError>>;

/// Connection attempts waiting for the test's decision.
#[derive(Debug, Clone, Default)]
pub struct FakeNet {
    attempts: Arc<Mutex<VecDeque<Answer>>>,
    count: Arc<Mutex<usize>>,
}

impl FakeNet {
    pub fn connector(&self) -> impl Connector<Transport = FakeTransport> + use<> {
        let net = self.clone();
        move || {
            let (answer, decision) = oneshot::channel();
            net.attempts
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_back(answer);
            *net.count.lock().unwrap_or_else(PoisonError::into_inner) += 1;
            async move {
                decision
                    .await
                    .unwrap_or_else(|_| Err(TransportError("attempt dropped".into())))
            }
        }
    }

    /// Connection attempts so far.
    pub fn attempts(&self) -> usize {
        *self.count.lock().unwrap()
    }

    pub fn take_attempt(&self) -> Option<Attempt> {
        self.attempts.lock().unwrap().pop_front().map(Attempt)
    }
}

/// One connection attempt.
#[derive(Debug)]
pub struct Attempt(Answer);

impl Attempt {
    pub fn accept(self) -> ServerEnd {
        let (to_server, from_client) = mpsc::unbounded();
        let (to_client, from_server) = mpsc::unbounded();
        let send_budget = Arc::new(Mutex::new(None));
        let transport = FakeTransport {
            to_server: Some(to_server),
            from_server,
            closed: None,
            send_budget: Arc::clone(&send_budget),
        };
        self.0.send(Ok(transport)).unwrap();
        ServerEnd {
            from_client,
            to_client,
            send_budget,
        }
    }

    pub fn refuse(self, message: &str) {
        self.0.send(Err(TransportError(message.into()))).unwrap();
    }
}

/// The client's side of an accepted fake connection.
#[derive(Debug)]
pub struct FakeTransport {
    to_server: Option<mpsc::UnboundedSender<String>>,
    from_server: mpsc::UnboundedReceiver<Incoming>,
    closed: Option<Closed>,
    /// Sends that still succeed; `None`: unlimited.
    send_budget: Arc<Mutex<Option<usize>>>,
}

impl Transport for FakeTransport {
    fn send(&mut self, text: String) -> impl Future<Output = Result<(), TransportError>> {
        let allowed = match &mut *self.send_budget.lock().unwrap() {
            Some(0) => false,
            Some(left) => {
                *left -= 1;
                true
            }
            None => true,
        };
        std::future::ready(if allowed {
            self.to_server
                .as_ref()
                .and_then(|sender| sender.unbounded_send(text).ok())
                .ok_or_else(|| TransportError("closed".into()))
        } else {
            Err(TransportError("send failed".into()))
        })
    }

    async fn recv(&mut self) -> Incoming {
        if let Some(closed) = &self.closed {
            return Incoming::Closed(closed.clone());
        }
        let incoming = self.from_server.next().await.unwrap_or_else(|| {
            Incoming::Closed(Closed {
                code: None,
                reason: "server end dropped".into(),
            })
        });
        if let Incoming::Closed(closed) = &incoming {
            self.closed = Some(closed.clone());
        }
        incoming
    }

    fn close(&mut self) -> impl Future<Output = ()> {
        self.to_server = None;
        std::future::ready(())
    }
}

/// The daemon's side of a fake connection.
#[derive(Debug)]
pub struct ServerEnd {
    from_client: mpsc::UnboundedReceiver<String>,
    to_client: mpsc::UnboundedSender<Incoming>,
    send_budget: Arc<Mutex<Option<usize>>>,
}

impl ServerEnd {
    /// The client's next `sends` sends succeed; every later one fails with a transport error.
    pub fn fail_sends_after(&self, sends: usize) {
        *self.send_budget.lock().unwrap() = Some(sends);
    }

    /// Frames the client sent since the last call.
    pub fn frames(&mut self) -> Vec<Value> {
        let mut frames = Vec::new();
        while let Ok(text) = self.from_client.try_recv() {
            frames.push(serde_json::from_str(&text).unwrap());
        }
        frames
    }

    /// Whether the client closed its side.
    pub fn closed_by_client(&mut self) -> bool {
        matches!(self.from_client.try_recv(), Err(mpsc::TryRecvError::Closed))
    }

    pub fn send(&self, frame: &Value) {
        self.to_client
            .unbounded_send(Incoming::Text(frame.to_string()))
            .unwrap();
    }

    pub fn respond(&self, id: &Value, result: &Value) {
        self.send(&json!({ "type": "response", "id": id, "ok": true, "result": result }));
    }

    pub fn fail(&self, id: &Value, error: &Value) {
        self.send(&json!({ "type": "response", "id": id, "ok": false, "error": error }));
    }

    pub fn close(&self, code: Option<u16>) {
        let _ = self.to_client.unbounded_send(Incoming::Closed(Closed {
            code,
            reason: String::new(),
        }));
    }
}

/// A timer whose sleeps end when the test fires them.
#[derive(Debug, Clone, Default)]
pub struct ManualTimer {
    state: Arc<Mutex<TimerState>>,
}

#[derive(Debug, Default)]
struct TimerState {
    requested: Vec<Duration>,
    pending: Vec<oneshot::Sender<()>>,
}

impl ManualTimer {
    /// Every sleep requested so far, in order.
    pub fn requested(&self) -> Vec<Duration> {
        self.state.lock().unwrap().requested.clone()
    }

    /// Ends every pending sleep.
    pub fn fire(&self) {
        for sleep in self.state.lock().unwrap().pending.drain(..) {
            let _ = sleep.send(());
        }
    }
}

impl Timer for ManualTimer {
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> {
        let (done, fired) = oneshot::channel();
        let mut state = self.state.lock().unwrap();
        state.requested.push(duration);
        state.pending.push(done);
        async move {
            let _ = fired.await;
        }
    }
}

pub const TOKEN: &str = "secret";

/// A client on the fake network, driven on a single-threaded pool the test steps until stalled.
pub struct Rig {
    pool: LocalPool,
    pub net: FakeNet,
    pub timer: ManualTimer,
    pub client: Client,
    done: Rc<RefCell<bool>>,
}

impl Rig {
    pub fn new() -> Self {
        let net = FakeNet::default();
        let timer = ManualTimer::default();
        let (client, driver) = connect(
            net.connector(),
            timer.clone(),
            ClientConfig::new(TOKEN, "test/l1"),
        );
        let pool = LocalPool::new();
        let done = Rc::new(RefCell::new(false));
        let finished = Rc::clone(&done);
        pool.spawner()
            .spawn_local(async move {
                driver.await;
                *finished.borrow_mut() = true;
            })
            .unwrap();
        let mut rig = Self {
            pool,
            net,
            timer,
            client,
            done,
        };
        rig.run();
        rig
    }

    /// Runs the driver (and spawned calls) until nothing can progress.
    pub fn run(&mut self) {
        self.pool.run_until_stalled();
    }

    pub fn driver_done(&self) -> bool {
        *self.done.borrow()
    }

    pub fn status(&self) -> ConnectionStatus {
        self.client.read(|model| model.connection().clone())
    }

    /// Accepts the pending connection attempt.
    pub fn accept(&mut self) -> ServerEnd {
        let server = self
            .net
            .take_attempt()
            .expect("a connection attempt")
            .accept();
        self.run();
        server
    }

    /// Accepts the pending attempt and answers its hello; returns the connection and the frames sent after hello.
    pub fn connect(&mut self) -> (ServerEnd, Vec<Value>) {
        let mut server = self.accept();
        let mut frames = server.frames();
        let hello = frames.remove(0);
        assert_eq!(hello["method"], "system.hello");
        server.respond(&hello["id"], &hello_result());
        self.run();
        frames.extend(server.frames());
        (server, frames)
    }

    /// Starts a call; its result appears in the returned cell once answered.
    pub fn call(
        &mut self,
        call: Call,
    ) -> Rc<RefCell<Option<Result<Value, omfp_client::CallError>>>> {
        let cell = Rc::new(RefCell::new(None));
        let result = Rc::clone(&cell);
        let response = self.client.call(call);
        self.pool
            .spawner()
            .spawn_local(async move {
                *result.borrow_mut() = Some(response.await);
            })
            .unwrap();
        self.run();
        cell
    }
}

pub fn hello_result() -> Value {
    json!({ "protocol": 1, "daemon": "0.1.0", "omp": omfp_core::OMP_VERSION, "warnings": [] })
}
