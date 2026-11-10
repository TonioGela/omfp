//! The WebSocket server (03-protocol): `/api/ws` for clients, the user's theme at `/theme.css` (DMN-075), and the
//! web UI's static files at `/` when `--static-dir` is set.

mod auth;
mod connection;
mod convert;
mod dispatch;
mod services;

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::sync::watch;

pub use auth::{Permission, Principal, authorize, permission};
pub(crate) use services::Services;

use crate::token::Token;

/// Protocol versions the daemon accepts in `system.hello`: the current one and, during a transition, the previous
/// one (PRT-032). There is no previous version yet.
pub const SUPPORTED_PROTOCOLS: &[u32] = &[omfp_protocol::PROTOCOL_VERSION];

/// Frames a client may have queued before its topics are dropped with `resync_required` (PRT-023).
pub(crate) const OUTBOUND_QUEUE: usize = 1024;

/// Everything a connection needs.
pub(crate) struct AppState {
    pub token: Token,
    /// omp version reported in `system.hello` (PRT-005).
    pub omp_version: String,
    /// Startup warnings reported in `system.hello`, e.g. `--allow-omp-mismatch` (DMN-001).
    pub warnings: Vec<String>,
    pub allowed_origins: Vec<String>,
    pub static_dir: Option<PathBuf>,
    pub services: Services,
    /// Becomes `true` when the daemon shuts down; connections then close.
    pub shutdown: watch::Receiver<bool>,
}

pub(crate) fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/ws", get(upgrade))
        .route("/theme.css", get(theme_css))
        .fallback(static_file)
        .with_state(state)
}

/// The user's theme CSS (DMN-075): empty when none is set, never cached (a change must show on reload), and served
/// without the token like the bundle.
async fn theme_css(State(state): State<Arc<AppState>>) -> Response {
    let css = state.services.theme().css.unwrap_or_default();
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        css,
    )
        .into_response()
}

/// Whether a WebSocket upgrade with this `Origin` may proceed (PRT-003): no `Origin` (not a browser), the daemon's
/// own origin (`http(s)://<Host>`), or one of the configured origins.
#[must_use]
pub fn origin_allowed(origin: Option<&str>, host: Option<&str>, allowed: &[String]) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    allowed.iter().any(|allowed| allowed == origin)
        || host.is_some_and(|host| {
            origin == format!("http://{host}") || origin == format!("https://{host}")
        })
}

async fn upgrade(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    let header = |name| headers.get(name).and_then(|value| value.to_str().ok());
    if !origin_allowed(
        header(header::ORIGIN),
        header(header::HOST),
        &state.allowed_origins,
    ) {
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    upgrade
        .max_message_size(omfp_protocol::MAX_FRAME_BYTES)
        .max_frame_size(omfp_protocol::MAX_FRAME_BYTES)
        .on_upgrade(move |socket| connection::serve(socket, state))
}

/// Serves `--static-dir`; unknown paths get `index.html` so the UI can route them.
async fn static_file(State(state): State<Arc<AppState>>, request: Request) -> Response {
    let Some(root) = state.static_dir.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(relative) = safe_relative(request.uri().path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let read = tokio::task::spawn_blocking(move || {
        let path = root.join(&relative);
        let path = if path.is_file() {
            path
        } else {
            root.join("index.html")
        };
        std::fs::read(&path).map(|bytes| (bytes, content_type(&path)))
    })
    .await;
    match read {
        Ok(Ok((bytes, content_type))) => {
            ([(header::CONTENT_TYPE, content_type)], bytes).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The request path as a relative path without `..`; `None` when it tries to leave the root.
fn safe_relative(path: &str) -> Option<PathBuf> {
    let mut relative = PathBuf::new();
    for component in Path::new(path.trim_start_matches('/')).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(relative)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: PRT-003
    #[test]
    fn only_the_own_origin_and_configured_origins_may_upgrade() {
        let allowed = vec!["https://omfp.example.com".to_owned()];
        let host = Some("127.0.0.1:7420");

        assert!(origin_allowed(None, host, &allowed));
        assert!(origin_allowed(
            Some("http://127.0.0.1:7420"),
            host,
            &allowed
        ));
        assert!(origin_allowed(
            Some("https://omfp.example.com"),
            host,
            &allowed
        ));
        assert!(!origin_allowed(
            Some("https://evil.example"),
            host,
            &allowed
        ));
        assert!(!origin_allowed(
            Some("http://127.0.0.1:7421"),
            host,
            &allowed
        ));
        assert!(!origin_allowed(Some("http://127.0.0.1:7420"), None, &[]));
    }

    #[test]
    fn static_paths_cannot_leave_the_root() {
        assert_eq!(safe_relative("/"), Some(PathBuf::new()));
        assert_eq!(
            safe_relative("/assets/app.js"),
            Some(PathBuf::from("assets/app.js"))
        );
        assert_eq!(safe_relative("/../etc/passwd"), None);
        assert_eq!(safe_relative("/a/../../b"), None);
    }
}
