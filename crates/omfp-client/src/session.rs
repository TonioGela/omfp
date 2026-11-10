//! The protocol session (03-protocol): `system.hello` with the token (PRT-001, PRT-002), request/response
//! correlation by id (PRT-010), subscriptions resumed from their `{epoch, seq}` (PRT-020..024), and reconnect with
//! exponential backoff capped at 10 s (UI-003). A refused token (close 4401) or protocol version (PRT-004) stops the
//! client instead of retrying.
//!
//! [`connect`] returns a [`Client`] handle and the driver future that owns the connection; the host spawns it.

use std::collections::HashMap;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use futures::future::{Either, select};
use omfp_protocol::{
    Call, ErrorBody, ErrorCode, HelloParams, HelloResult, Method, PROTOCOL_VERSION, Request,
    Response, ServerFrame, SubscribeResult, Topic, TopicParams,
};
use serde_json::Value;

use crate::model::{ConnectionStatus, Dirty, Model, SubscribeError};
use crate::notify::{Changes, Shared};
use crate::timer::Timer;
use crate::transport::{Closed, Connector, Incoming, Transport, TransportError};

/// Close code of a connection that did not authenticate (PRT-001).
pub const CLOSE_UNAUTHORIZED: u16 = 4401;
/// Close code after `version_mismatch` (PRT-004).
pub const CLOSE_VERSION_MISMATCH: u16 = 4426;

/// Delay before reconnect attempt `n`: `initial · 2^(n-1)`, at most `max`, and never more than [`Backoff::CAP`]
/// (UI-003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
}

impl Backoff {
    /// Upper bound of every delay (UI-003).
    pub const CAP: Duration = Duration::from_secs(10);

    /// Delay before attempt `attempt` (1 for the first retry).
    #[must_use]
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 2_u32.saturating_pow(attempt.saturating_sub(1));
        self.initial
            .saturating_mul(factor)
            .min(self.max)
            .min(Self::CAP)
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_millis(500),
            max: Self::CAP,
        }
    }
}

/// What the client says in `system.hello`, and how it retries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    /// The daemon's token; sent in `system.hello`, never in the URL (PRT-002).
    pub token: String,
    /// Client name recorded as provenance (PRT-033), e.g. `omfp-ui/web`.
    pub client: String,
    pub backoff: Backoff,
}

impl ClientConfig {
    pub fn new(token: impl Into<String>, client: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            client: client.into(),
            backoff: Backoff::default(),
        }
    }
}

/// Why a call has no result.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    /// The daemon answered with an error.
    #[error("{} ({:?})", .0.message, .0.code)]
    Daemon(ErrorBody),
    /// The request was sent, but the connection dropped before its response; it may or may not have run.
    #[error("the connection was lost before the response arrived")]
    ConnectionLost,
    /// The client stopped (closed, unauthenticated or refused version) before the call could be sent.
    #[error("the client is closed")]
    Closed,
    /// `system.hello`, `subscribe` and `unsubscribe` belong to the session; use [`Client::subscribe`].
    #[error("`{0}` is managed by the client session")]
    Reserved(&'static str),
    /// The params did not encode or the result did not decode.
    #[error("{0}")]
    Codec(String),
}

type Responder = oneshot::Sender<Result<Value, CallError>>;

#[derive(Debug)]
enum Command {
    Call(Call, Responder),
    Subscribe(Topic),
    Unsubscribe(Topic),
    Close,
}

/// Handle of a running client; cheap to clone. Dropping every handle stops the driver.
#[derive(Debug, Clone)]
pub struct Client {
    commands: mpsc::UnboundedSender<Command>,
    shared: Arc<Shared>,
}

impl Client {
    /// Sends `call` (now, or once connected) and resolves to its result. The future does not borrow the client.
    pub fn call(&self, call: Call) -> impl Future<Output = Result<Value, CallError>> + use<> {
        let sent = if matches!(
            call,
            Call::SystemHello(_) | Call::Subscribe(_) | Call::Unsubscribe(_)
        ) {
            Err(CallError::Reserved(call.method()))
        } else {
            let (responder, response) = oneshot::channel();
            self.commands
                .unbounded_send(Command::Call(call, responder))
                .map(|()| response)
                .map_err(|_| CallError::Closed)
        };
        async move {
            match sent {
                Ok(response) => response.await.unwrap_or(Err(CallError::Closed)),
                Err(error) => Err(error),
            }
        }
    }

    /// [`Client::call`] with typed params and result.
    pub fn request<M: Method>(
        &self,
        params: M::Params,
    ) -> impl Future<Output = Result<M::Result, CallError>> + use<M> {
        let response = self.call(M::call(params));
        async move {
            let value = response.await?;
            serde_json::from_value(value)
                .map_err(|error| CallError::Codec(format!("invalid {} result: {error}", M::NAME)))
        }
    }

    /// Follows `topic`: subscribed now if connected, and resubscribed from its last position after every reconnect.
    pub fn subscribe(&self, topic: Topic) {
        let _ = self.commands.unbounded_send(Command::Subscribe(topic));
    }

    /// Stops following `topic` and drops its data from the model.
    pub fn unsubscribe(&self, topic: Topic) {
        let _ = self.commands.unbounded_send(Command::Unsubscribe(topic));
    }

    /// Closes the connection and stops the driver; pending calls fail.
    pub fn close(&self) {
        let _ = self.commands.unbounded_send(Command::Close);
    }

    /// Reads the model.
    pub fn read<R>(&self, read: impl FnOnce(&Model) -> R) -> R {
        self.shared.read(read)
    }

    /// What changed since the previous call (see [`Model::take_dirty`]).
    #[must_use]
    pub fn take_dirty(&self) -> Dirty {
        self.shared.take_dirty()
    }

    /// Wakes the host after model changes, coalesced (UI-012).
    #[must_use]
    pub fn changes(&self) -> Changes {
        Changes::new(Arc::clone(&self.shared))
    }
}

/// Creates a client: the handle, and the driver future the host spawns (tokio natively, `spawn_local` on the web).
/// The driver connects through `connector`, sleeps through `timer`, and ends when the client is closed, every handle
/// is dropped, or the daemon refuses the token or protocol version.
pub fn connect<C: Connector, T: Timer>(
    connector: C,
    timer: T,
    config: ClientConfig,
) -> (Client, impl Future<Output = ()>) {
    let (commands_tx, commands) = mpsc::unbounded();
    let shared = Arc::new(Shared::default());
    let client = Client {
        commands: commands_tx,
        shared: Arc::clone(&shared),
    };
    let session = Session {
        connector,
        timer,
        config,
        inbox: Inbox {
            commands,
            shared,
            queued: Vec::new(),
        },
    };
    (client, session.run())
}

struct Session<C, T> {
    connector: C,
    timer: T,
    config: ClientConfig,
    inbox: Inbox,
}

/// Commands and the model, shared by every connection of a session.
struct Inbox {
    commands: mpsc::UnboundedReceiver<Command>,
    shared: Arc<Shared>,
    /// Calls waiting for a connection; never sent twice.
    queued: Vec<(Call, Responder)>,
}

impl Inbox {
    /// Handles a command while no connection is ready; `false` when the client must stop.
    fn offline(&mut self, command: Option<Command>) -> bool {
        match command {
            Some(Command::Call(call, responder)) => self.queued.push((call, responder)),
            Some(Command::Subscribe(topic)) => {
                self.shared.update(|model| model.track(topic));
            }
            Some(Command::Unsubscribe(topic)) => {
                self.shared.update(|model| model.untrack(&topic));
            }
            Some(Command::Close) | None => return false,
        }
        true
    }

    /// Runs `future` while handling commands offline; `None` when the client must stop first.
    async fn until<F: Future>(&mut self, future: F) -> Option<F::Output> {
        let mut future = pin!(future);
        loop {
            match select(future.as_mut(), self.commands.next()).await {
                Either::Left((output, _)) => return Some(output),
                Either::Right((command, _)) => {
                    if !self.offline(command) {
                        return None;
                    }
                }
            }
        }
    }
}

/// How a connection ended.
enum Outcome {
    /// Retry after a backoff; `ready` when the connection had completed its hello.
    Lost { error: String, ready: bool },
    /// Do not retry.
    Stop(ConnectionStatus),
}

impl<C: Connector, T: Timer> Session<C, T> {
    async fn run(mut self) {
        let mut attempt = 0;
        let status = loop {
            let Some(connected) = self.inbox.until(self.connector.connect()).await else {
                break ConnectionStatus::Closed;
            };
            let outcome = match connected {
                Ok(transport) => {
                    Link::new(transport)
                        .serve(&mut self.inbox, &self.config)
                        .await
                }
                Err(error) => Outcome::Lost {
                    error: error.to_string(),
                    ready: false,
                },
            };
            let (error, ready) = match outcome {
                Outcome::Stop(status) => break status,
                Outcome::Lost { error, ready } => (error, ready),
            };
            if ready {
                attempt = 0;
            }
            attempt += 1;
            let retry_in = self.config.backoff.delay(attempt);
            self.inbox.shared.update(|model| {
                model.connection_lost();
                model.set_connection(ConnectionStatus::Reconnecting {
                    attempt,
                    retry_in,
                    error,
                });
            });
            if self.inbox.until(self.timer.sleep(retry_in)).await.is_none() {
                break ConnectionStatus::Closed;
            }
        };
        self.inbox.shared.update(|model| {
            model.connection_lost();
            model.set_connection(status);
        });
        for (_, responder) in self.inbox.queued.drain(..) {
            let _ = responder.send(Err(CallError::Closed));
        }
    }
}

/// What a sent request waits for.
#[derive(Debug)]
enum Pending {
    Hello,
    Call(Responder),
    Subscribe(Topic),
    Unsubscribe,
}

/// One connection.
struct Link<T> {
    transport: T,
    next_id: u64,
    /// `system.hello` succeeded.
    ready: bool,
    pending: HashMap<String, Pending>,
    /// Latest subscribe request per topic; older answers are stale.
    subscribing: HashMap<Topic, String>,
}

impl<T: Transport> Link<T> {
    fn new(transport: T) -> Self {
        Self {
            transport,
            next_id: 1,
            ready: false,
            pending: HashMap::new(),
            subscribing: HashMap::new(),
        }
    }

    async fn serve(mut self, inbox: &mut Inbox, config: &ClientConfig) -> Outcome {
        let outcome = self.run(inbox, config).await;
        for (_, pending) in self.pending.drain() {
            if let Pending::Call(responder) = pending {
                let _ = responder.send(Err(CallError::ConnectionLost));
            }
        }
        outcome
    }

    fn lost(&self, error: &TransportError) -> Outcome {
        Outcome::Lost {
            error: error.to_string(),
            ready: self.ready,
        }
    }

    async fn run(&mut self, inbox: &mut Inbox, config: &ClientConfig) -> Outcome {
        let hello = Call::SystemHello(HelloParams {
            token: config.token.clone(),
            protocol: PROTOCOL_VERSION,
            client: config.client.clone(),
        });
        if let Err(error) = self.send(hello, Pending::Hello, inbox).await {
            return self.lost(&error);
        }
        loop {
            let next = {
                let incoming = pin!(self.transport.recv());
                match select(incoming, inbox.commands.next()).await {
                    Either::Left((incoming, _)) => Either::Left(incoming),
                    Either::Right((command, _)) => Either::Right(command),
                }
            };
            let result = match next {
                Either::Left(Incoming::Text(text)) => self.on_text(&text, inbox).await,
                Either::Left(Incoming::Closed(closed)) => return self.on_closed(&closed),
                Either::Right(Some(Command::Close) | None) => {
                    self.transport.close().await;
                    return Outcome::Stop(ConnectionStatus::Closed);
                }
                Either::Right(Some(command)) => {
                    self.on_command(command, inbox).await.map(|()| None)
                }
            };
            match result {
                Ok(None) => {}
                Ok(Some(outcome)) => return outcome,
                Err(error) => return self.lost(&error),
            }
        }
    }

    /// Sends a request; its response is routed by `pending`. Returns the request id. A transport error means the
    /// frame was not delivered: a call goes back to the queue for the next connection.
    async fn send(
        &mut self,
        call: Call,
        pending: Pending,
        inbox: &mut Inbox,
    ) -> Result<String, TransportError> {
        let id = format!("c{}", self.next_id);
        self.next_id += 1;
        let request = Request::new(id.clone(), call);
        let text = match serde_json::to_string(&request) {
            Ok(text) => text,
            Err(error) => {
                if let Pending::Call(responder) = pending {
                    let _ = responder.send(Err(CallError::Codec(error.to_string())));
                }
                return Ok(id);
            }
        };
        self.pending.insert(id.clone(), pending);
        if let Err(error) = self.transport.send(text).await {
            if let Some(Pending::Call(responder)) = self.pending.remove(&id) {
                inbox.queued.push((request.call, responder));
            }
            return Err(error);
        }
        Ok(id)
    }

    /// (Re)subscribes `topic` from its last position.
    async fn subscribe(&mut self, topic: Topic, inbox: &mut Inbox) -> Result<(), TransportError> {
        let Some(params) = inbox.shared.update(|model| model.subscribe_params(&topic)) else {
            return Ok(());
        };
        let id = self
            .send(
                Call::Subscribe(params),
                Pending::Subscribe(topic.clone()),
                inbox,
            )
            .await?;
        self.subscribing.insert(topic, id);
        Ok(())
    }

    /// Asks for a snapshot of `topic` (PRT-023), unless a subscribe request is already in flight: its answer
    /// starts a stream the daemon still delivers.
    async fn resync(&mut self, topic: Topic, inbox: &mut Inbox) -> Result<(), TransportError> {
        if !self.subscribing.contains_key(&topic)
            && inbox.shared.update(|model| model.resync(&topic))
        {
            self.subscribe(topic, inbox).await?;
        }
        Ok(())
    }

    async fn on_command(
        &mut self,
        command: Command,
        inbox: &mut Inbox,
    ) -> Result<(), TransportError> {
        if !self.ready {
            inbox.offline(Some(command));
            return Ok(());
        }
        match command {
            Command::Call(call, responder) => {
                self.send(call, Pending::Call(responder), inbox).await?;
            }
            Command::Subscribe(topic) => {
                if inbox.shared.update(|model| model.track(topic.clone())) {
                    self.subscribe(topic, inbox).await?;
                }
            }
            Command::Unsubscribe(topic) => {
                self.subscribing.remove(&topic);
                if inbox.shared.update(|model| model.untrack(&topic)) {
                    let call = Call::Unsubscribe(TopicParams { topic });
                    self.send(call, Pending::Unsubscribe, inbox).await?;
                }
            }
            Command::Close => {}
        }
        Ok(())
    }

    /// Handles one daemon frame.
    async fn on_text(
        &mut self,
        text: &str,
        inbox: &mut Inbox,
    ) -> Result<Option<Outcome>, TransportError> {
        let frame = match serde_json::from_str::<ServerFrame>(text) {
            Ok(frame) => frame,
            Err(error) => return self.on_malformed(text, &error, inbox).await.map(|()| None),
        };
        match frame {
            ServerFrame::Response(response) => return self.on_response(response, inbox).await,
            ServerFrame::Event(event) => {
                let applied = inbox.shared.update(|model| model.apply_event(&event));
                if applied == crate::model::Applied::Gap {
                    self.subscribe(event.topic, inbox).await?;
                }
            }
            ServerFrame::ResyncRequired { topic } => self.resync(topic, inbox).await?,
        }
        Ok(None)
    }

    /// A frame that does not decode: the request it answers fails (a bad hello drops the connection), and an event
    /// of a followed topic resyncs that topic, since the stream can no longer be continued.
    async fn on_malformed(
        &mut self,
        text: &str,
        error: &serde_json::Error,
        inbox: &mut Inbox,
    ) -> Result<(), TransportError> {
        let Ok(frame) = serde_json::from_str::<Value>(text) else {
            return Ok(());
        };
        let field = |name: &str| frame.get(name).and_then(Value::as_str);
        match field("type") {
            Some("response") => {
                let Some(id) = field("id") else {
                    return Ok(());
                };
                match self.pending.remove(id) {
                    Some(Pending::Hello) => {
                        return Err(TransportError(format!("invalid hello answer: {error}")));
                    }
                    Some(Pending::Call(responder)) => {
                        let error = CallError::Codec(format!("invalid response: {error}"));
                        let _ = responder.send(Err(error));
                    }
                    Some(Pending::Subscribe(topic)) => {
                        if self.subscribing.get(&topic).map(String::as_str) == Some(id) {
                            self.subscribing.remove(&topic);
                            let error = ErrorBody::new(
                                ErrorCode::Internal,
                                format!("invalid subscribe answer: {error}"),
                            );
                            inbox.shared.update(|model| model.fail_topic(&topic, error));
                        }
                    }
                    Some(Pending::Unsubscribe) | None => {}
                }
            }
            Some("event") => {
                if let Some(topic) = field("topic").and_then(|topic| topic.parse::<Topic>().ok()) {
                    self.resync(topic, inbox).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn on_response(
        &mut self,
        response: Response,
        inbox: &mut Inbox,
    ) -> Result<Option<Outcome>, TransportError> {
        let Some(pending) = self.pending.remove(&response.id) else {
            return Ok(None);
        };
        match pending {
            Pending::Hello => return self.on_hello(response.result, inbox).await,
            Pending::Call(responder) => {
                let _ = responder.send(response.result.map_err(CallError::Daemon));
            }
            Pending::Subscribe(topic) => {
                if self.subscribing.get(&topic) == Some(&response.id) {
                    self.subscribing.remove(&topic);
                    self.on_subscribed(topic, response.result, inbox).await?;
                }
            }
            Pending::Unsubscribe => {}
        }
        Ok(None)
    }

    async fn on_subscribed(
        &mut self,
        topic: Topic,
        result: Result<Value, ErrorBody>,
        inbox: &mut Inbox,
    ) -> Result<(), TransportError> {
        let decoded = result.and_then(|value| {
            serde_json::from_value::<SubscribeResult>(value).map_err(|error| {
                ErrorBody::new(
                    ErrorCode::Internal,
                    format!("invalid subscribe result: {error}"),
                )
            })
        });
        match decoded {
            Ok(result) => {
                let applied = inbox
                    .shared
                    .update(|model| model.apply_subscribed(&topic, result));
                if let Err(SubscribeError::ReplayMismatch { .. }) = applied {
                    self.subscribe(topic, inbox).await?;
                }
            }
            Err(error) => inbox.shared.update(|model| model.fail_topic(&topic, error)),
        }
        Ok(())
    }

    async fn on_hello(
        &mut self,
        result: Result<Value, ErrorBody>,
        inbox: &mut Inbox,
    ) -> Result<Option<Outcome>, TransportError> {
        let hello = match result {
            Ok(value) => serde_json::from_value::<HelloResult>(value)
                .map_err(|error| TransportError(format!("invalid hello result: {error}")))?,
            Err(error) => {
                self.transport.close().await;
                return Ok(Some(refused(error)));
            }
        };
        self.ready = true;
        let topics: Vec<Topic> = inbox.shared.update(|model| {
            model.set_connection(ConnectionStatus::Connected(hello));
            model
                .topics()
                .iter()
                .map(|state| state.topic.clone())
                .collect()
        });
        for topic in topics {
            self.subscribe(topic, inbox).await?;
        }
        let mut queued = std::mem::take(&mut inbox.queued).into_iter();
        while let Some((call, responder)) = queued.next() {
            if let Err(error) = self.send(call, Pending::Call(responder), inbox).await {
                // The failed call went back to the queue; the rest follows it.
                inbox.queued.extend(queued);
                return Err(error);
            }
        }
        Ok(None)
    }

    fn on_closed(&self, closed: &Closed) -> Outcome {
        match closed.code {
            Some(CLOSE_UNAUTHORIZED) => Outcome::Stop(ConnectionStatus::Unauthenticated),
            Some(CLOSE_VERSION_MISMATCH) => Outcome::Stop(ConnectionStatus::VersionMismatch {
                supported: Vec::new(),
                message: closed.reason.clone(),
            }),
            _ => Outcome::Lost {
                error: closed.to_string(),
                ready: self.ready,
            },
        }
    }
}

/// The outcome of a failed `system.hello`.
fn refused(error: ErrorBody) -> Outcome {
    match error.code {
        ErrorCode::Unauthorized => Outcome::Stop(ConnectionStatus::Unauthenticated),
        ErrorCode::VersionMismatch => {
            let supported = error
                .details
                .as_ref()
                .and_then(|details| details.get("supported"))
                .and_then(|supported| serde_json::from_value(supported.clone()).ok())
                .unwrap_or_default();
            Outcome::Stop(ConnectionStatus::VersionMismatch {
                supported,
                message: error.message,
            })
        }
        _ => Outcome::Lost {
            error: error.message,
            ready: false,
        },
    }
}
