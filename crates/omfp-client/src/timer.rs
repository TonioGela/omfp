//! The clock port: the session sleeps through a [`Timer`] (tokio natively, gloo-timers in the browser, a manual fake in
//! tests), so reconnect backoff is testable without real time (TST-003).

use std::time::Duration;

/// Sleeps.
pub trait Timer {
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()>;
}

/// [`Timer`] on tokio's clock (honours `tokio::time::pause`).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioTimer;

#[cfg(not(target_arch = "wasm32"))]
impl Timer for TokioTimer {
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> {
        tokio::time::sleep(duration)
    }
}

/// [`Timer`] on the browser's `setTimeout`.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Copy, Default)]
pub struct GlooTimer;

#[cfg(target_arch = "wasm32")]
impl Timer for GlooTimer {
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> {
        gloo_timers::future::sleep(duration)
    }
}
