//! The connection banner (UI-003) and the status line announced through the ARIA live region (UI-092).

use omfp_protocol::ThreadSummary;

use super::threads::ThreadStatus;
use crate::input::ConnectionView;

/// Tone of the banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Warning,
    Error,
}

impl Tone {
    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// The non-blocking banner shown while not connected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    pub tone: Tone,
    pub text: String,
}

/// The banner for `connection`; none once connected.
#[must_use]
pub fn banner(connection: &ConnectionView) -> Option<Banner> {
    let (tone, text) = match connection {
        ConnectionView::Connected => return None,
        ConnectionView::Connecting => (Tone::Info, "Connecting to the daemon…".to_owned()),
        ConnectionView::Reconnecting {
            attempt,
            retry_in_ms,
        } => (
            Tone::Warning,
            format!(
                "Connection lost. Reconnecting in {} s (attempt {attempt})…",
                retry_in_ms.div_ceil(1000)
            ),
        ),
        ConnectionView::Failed { message } => {
            (Tone::Error, format!("Cannot reach the daemon: {message}"))
        }
        ConnectionView::Unauthenticated => (
            Tone::Error,
            "The daemon rejected the access token. Open the link printed by `omfp daemon`."
                .to_owned(),
        ),
    };
    Some(Banner { tone, text })
}

/// The status line for screen readers: connection trouble, the selected thread's state, and how many threads wait
/// for input. Changes of this text are announced (UI-092).
#[must_use]
pub fn announcement(
    connection: &ConnectionView,
    threads: &[ThreadSummary],
    selected: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    if let Some(banner) = banner(connection) {
        parts.push(banner.text);
    }
    if let Some(thread) = selected.and_then(|id| threads.iter().find(|thread| thread.id == id)) {
        parts.push(format!(
            "{}: {}",
            thread.title,
            ThreadStatus::of(thread).label()
        ));
    }
    let waiting = threads
        .iter()
        .filter(|thread| thread.archived_at.is_none() && thread.needs_attention)
        .count();
    match waiting {
        0 => {}
        1 => parts.push("1 thread awaits input".to_owned()),
        count => parts.push(format!("{count} threads await input")),
    }
    parts.join(". ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::thread;

    // spec: UI-003
    #[test]
    fn the_banner_shows_reconnect_progress_and_disappears_once_connected() {
        let banner = banner(&ConnectionView::Reconnecting {
            attempt: 3,
            retry_in_ms: 4_200,
        })
        .expect("banner");
        assert_eq!(
            (banner.tone, banner.text.as_str()),
            (
                Tone::Warning,
                "Connection lost. Reconnecting in 5 s (attempt 3)…"
            )
        );
        assert_eq!(super::banner(&ConnectionView::Connected), None);
        let failed = super::banner(&ConnectionView::Unauthenticated).expect("banner");
        assert_eq!(failed.tone, Tone::Error);
    }

    // spec: UI-092
    #[test]
    fn the_announcement_names_the_selected_thread_state_and_waiting_threads() {
        let mut first = thread("t1", "checkout", &[]);
        first.needs_attention = true;
        let mut second = thread("t2", "flaky", &[]);
        second.needs_attention = true;
        let threads = vec![first, second];
        assert_eq!(
            announcement(&ConnectionView::Connected, &threads, Some("t1")),
            "checkout: awaiting input. 2 threads await input"
        );
        assert_eq!(
            announcement(
                &ConnectionView::Reconnecting {
                    attempt: 1,
                    retry_in_ms: 1000
                },
                &threads[..0],
                None
            ),
            "Connection lost. Reconnecting in 1 s (attempt 1)…"
        );
    }
}
