//! Scripted OpenAI-compatible server for deterministic omp runs (docs/specs/06-testing.md, "Fake LLM").
//!
//! omp talks to it as a custom `openai-completions` provider. Each chat request consumes the next [`Step`] of the
//! [`Script`]; a request the script does not expect is answered with HTTP 500 and recorded in [`FakeLlm::failures`],
//! so the test can fail with the offending request in hand.

mod chunks;
mod script;

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub use script::{Matcher, Reply, Script, Step, ToolCall};

#[derive(Debug)]
struct Inner {
    model: String,
    steps: Mutex<VecDeque<Step>>,
    answered: Mutex<usize>,
    requests: Mutex<Vec<Value>>,
    failures: Mutex<Vec<String>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A running fake LLM bound to `127.0.0.1` on a free port. Stops when dropped.
#[derive(Debug)]
pub struct FakeLlm {
    addr: SocketAddr,
    inner: Arc<Inner>,
    task: JoinHandle<()>,
}

impl FakeLlm {
    /// Binds a free loopback port and serves `script` in the background.
    pub async fn start(script: Script) -> anyhow::Result<Self> {
        Self::start_on(script, SocketAddr::from(([127, 0, 0, 1], 0))).await
    }

    /// Serves `script` on `addr` (port 0 picks a free port).
    pub async fn start_on(script: Script, addr: SocketAddr) -> anyhow::Result<Self> {
        let (model, steps) = script.into_parts();
        let inner = Arc::new(Inner {
            model,
            steps: Mutex::new(steps.into()),
            answered: Mutex::new(0),
            requests: Mutex::new(Vec::new()),
            failures: Mutex::new(Vec::new()),
        });
        let router = Router::new()
            .route("/v1/models", get(models))
            .route("/v1/chat/completions", post(chat_completions))
            .route("/__requests", get(recorded_requests))
            .with_state(Arc::clone(&inner));
        let listener = TcpListener::bind(addr).await?;
        let addr = listener.local_addr()?;
        let task = tokio::spawn(async move {
            // The server only stops when the task is aborted on drop.
            let _ = axum::serve(listener, router).await;
        });
        Ok(Self { addr, inner, task })
    }

    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// OpenAI-style base URL, including `/v1`.
    #[must_use]
    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.addr)
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.inner.model
    }

    /// Bodies of every chat request received, in order.
    #[must_use]
    pub fn requests(&self) -> Vec<Value> {
        lock(&self.inner.requests).clone()
    }

    /// One message per request the script did not expect.
    #[must_use]
    pub fn failures(&self) -> Vec<String> {
        lock(&self.inner.failures).clone()
    }

    /// `true` when every step was consumed and no unexpected request arrived.
    #[must_use]
    pub fn is_exhausted_cleanly(&self) -> bool {
        lock(&self.inner.steps).is_empty() && lock(&self.inner.failures).is_empty()
    }
}

impl Drop for FakeLlm {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn models(State(inner): State<Arc<Inner>>) -> Response {
    axum::Json(json!({
        "object": "list",
        "data": [ { "id": inner.model, "object": "model", "created": 0, "owned_by": "fake-llm" } ],
    }))
    .into_response()
}

async fn recorded_requests(State(inner): State<Arc<Inner>>) -> Response {
    axum::Json(Value::Array(lock(&inner.requests).clone())).into_response()
}

async fn chat_completions(
    State(inner): State<Arc<Inner>>,
    axum::Json(request): axum::Json<Value>,
) -> Response {
    lock(&inner.requests).push(request.clone());

    let step = {
        let mut steps = lock(&inner.steps);
        match steps.front() {
            None => Err(format!(
                "unexpected request: script exhausted; request: {request}"
            )),
            Some(step) => match &step.when {
                Some(matcher) if !matcher.matches(&request) => Err(format!(
                    "request did not match {matcher:?}; last user message: {:?}; request: {request}",
                    script::last_user_text(&request)
                )),
                _ => Ok(steps.pop_front().expect("front exists")),
            },
        }
    };
    let step = match step {
        Ok(step) => step,
        Err(failure) => {
            lock(&inner.failures).push(failure.clone());
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": { "message": failure } })),
            )
                .into_response();
        }
    };

    let id = {
        let mut answered = lock(&inner.answered);
        *answered += 1;
        format!("chatcmpl-fake-{answered}")
    };
    if request["stream"] == Value::Bool(true) {
        let body = chunks::sse_body(&chunks::stream_chunks(
            &id,
            &inner.model,
            &request,
            &step.reply,
        ));
        (
            [
                (header::CONTENT_TYPE, "text/event-stream"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            body,
        )
            .into_response()
    } else {
        axum::Json(chunks::completion(&id, &inner.model, &request, &step.reply)).into_response()
    }
}
