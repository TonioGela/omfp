//! One WebSocket connection to the daemon's `/api/ws`, as text frames in and out (UI-001).
//!
//! Futures need not be `Send`: in the browser everything runs on one thread. Natively, [`NativeTransport`]'s futures
//! are `Send`, so a driver built on it can be spawned on a multi-threaded runtime.

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod web;

use std::fmt;

#[cfg(not(target_arch = "wasm32"))]
pub use native::NativeTransport;
#[cfg(target_arch = "wasm32")]
pub use web::WebTransport;

/// The connection could not be opened or used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct TransportError(pub String);

/// How a connection ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    /// WebSocket close code, when the peer sent one (4401: unauthenticated, 4426: version mismatch, 1001: daemon
    /// shutting down).
    pub code: Option<u16>,
    pub reason: String,
}

impl fmt::Display for Closed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.code, self.reason.is_empty()) {
            (Some(code), true) => write!(f, "connection closed ({code})"),
            (Some(code), false) => write!(f, "connection closed ({code}): {}", self.reason),
            (None, true) => f.write_str("connection lost"),
            (None, false) => write!(f, "connection lost: {}", self.reason),
        }
    }
}

/// What the daemon sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    /// One JSON text frame.
    Text(String),
    /// The connection is gone; every later `recv` returns this again.
    Closed(Closed),
}

/// An open connection.
pub trait Transport {
    /// Sends one text frame.
    fn send(&mut self, text: String) -> impl Future<Output = Result<(), TransportError>>;

    /// The next frame. Must be cancel-safe: dropping the future before it completes loses no frame.
    fn recv(&mut self) -> impl Future<Output = Incoming>;

    /// Closes the connection normally (1000).
    fn close(&mut self) -> impl Future<Output = ()>;
}

/// Opens connections; called again for every reconnect attempt.
///
/// Implemented by closures, e.g. `move || NativeTransport::connect(url.clone())`.
pub trait Connector {
    type Transport: Transport;

    fn connect(&self) -> impl Future<Output = Result<Self::Transport, TransportError>>;
}

impl<F, Fut, T> Connector for F
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, TransportError>>,
    T: Transport,
{
    type Transport = T;

    fn connect(&self) -> impl Future<Output = Result<T, TransportError>> {
        self()
    }
}
