//! Protocol client of the omfp daemon (docs/specs/04-ui.md, "State and code structure"), without any UI.
//!
//! - [`transport`]: the [`Transport`] trait with [`NativeTransport`] (tokio-tungstenite) and [`WebTransport`]
//!   (`web-sys` `WebSocket`), both checked by the same transport suite against a real daemon (UI-001, TST-030).
//! - [`session`]: `system.hello`, request/response correlation, subscriptions with `{epoch, seq}`, reconnect with
//!   capped exponential backoff and resubscription (UI-003).
//! - [`model`]: pure state kept from frames: connection status, index, per-thread `omfp_core::ThreadState`, with a
//!   [`Dirty`](model::Dirty) set the host drains once per frame (UI-012).
//!
//! The client is runtime-agnostic: [`connect`] returns a cheap [`Client`] handle and a driver future the host spawns
//! (a tokio task natively, `spawn_local` in the browser); sleeps go through a [`Timer`].
//!
//! ```no_run
//! # #[cfg(not(target_arch = "wasm32"))]
//! # async fn host() {
//! use omfp_client::{ClientConfig, NativeTransport, TokioTimer, connect};
//! use omfp_protocol::Topic;
//!
//! let url = "ws://127.0.0.1:7420/api/ws".to_owned();
//! let config = ClientConfig::new("<token>", "omfp-ui/desktop");
//! let (client, driver) = connect(move || NativeTransport::connect(url.clone()), TokioTimer, config);
//! tokio::spawn(driver);
//! client.subscribe(Topic::Index);
//! let mut changes = client.changes();
//! loop {
//!     changes.next().await; // at most once per frame
//!     let dirty = client.take_dirty();
//!     client.read(|model| println!("{:?}: {} threads ({dirty:?})", model.connection(), model.index().threads.len()));
//! }
//! # }
//! ```

pub mod model;
mod notify;
pub mod session;
pub mod timer;
pub mod transport;

pub use model::Model;
pub use notify::Changes;
pub use session::{Backoff, CallError, Client, ClientConfig, connect};
#[cfg(target_arch = "wasm32")]
pub use timer::GlooTimer;
pub use timer::Timer;
#[cfg(not(target_arch = "wasm32"))]
pub use timer::TokioTimer;
#[cfg(not(target_arch = "wasm32"))]
pub use transport::NativeTransport;
#[cfg(target_arch = "wasm32")]
pub use transport::WebTransport;
pub use transport::{Closed, Connector, Incoming, Transport, TransportError};
