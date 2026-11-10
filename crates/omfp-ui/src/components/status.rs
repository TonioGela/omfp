//! Badges, chips, the connection banner, the live region and the countdown.

use dioxus::prelude::*;

use crate::input::ConnectionView;
use crate::platform;
use crate::vm::connection::banner;
use crate::vm::interaction::{format_countdown, remaining_ms};
use crate::vm::threads::{ThreadStatus, WorkspaceChip};

/// A thread's state badge (UI-020).
#[component]
pub(crate) fn StatusBadge(status: ThreadStatus, testid: &'static str) -> Element {
    rsx! {
        span {
            class: "badge status-{status.token()}",
            "data-testid": testid,
            "data-status": status.token(),
            title: status.label(),
            span { "aria-hidden": "true", "{status.glyph()}" }
            span { class: "sr-only", "{status.label()}" }
        }
    }
}

/// `repo:branch #PR`, marked when it is the thread's cwd. With `link`, the PR number links to the PR.
#[component]
pub(crate) fn WorkspaceChipView(chip: WorkspaceChip, link: bool) -> Element {
    let title = if chip.cwd {
        "Working directory"
    } else {
        "Workspace"
    };
    rsx! {
        span {
            class: "chip",
            "data-testid": "workspace-chip",
            "data-workspace-id": "{chip.workspace_id}",
            "data-cwd": chip.cwd,
            title,
            if chip.cwd {
                span { class: "chip-cwd", "aria-label": "cwd", "⌂" }
            }
            "{chip.label}"
            if let Some(number) = chip.pr_number {
                match (link, &chip.pr_url) {
                    (true, Some(url)) => rsx! {
                        a { class: "chip-pr", href: "{url}", target: "_blank", rel: "noopener noreferrer", "#{number}" }
                    },
                    _ => rsx! {
                        span { class: "chip-pr", "#{number}" }
                    },
                }
            }
        }
    }
}

/// The non-blocking banner shown while not connected (UI-003).
#[component]
pub(crate) fn ConnectionBanner(connection: ConnectionView) -> Element {
    let Some(banner) = banner(&connection) else {
        return rsx! {};
    };
    rsx! {
        div { class: "banner tone-{banner.tone.token()}", "data-testid": "connection-banner", "data-tone": banner.tone.token(),
            "{banner.text}"
        }
    }
}

/// Status changes for assistive technology (UI-092).
#[component]
pub(crate) fn LiveRegion(text: String) -> Element {
    rsx! {
        div { class: "sr-only", role: "status", "aria-live": "polite", "aria-atomic": "true", "data-testid": "live-region",
            "{text}"
        }
    }
}

/// Time left before a request times out, ticking every second (UI-073).
#[component]
pub(crate) fn Countdown(timeout_ms: i64, shown_at: i64) -> Element {
    let mut now = use_signal(|| platform::now_ms().max(shown_at));
    use_future(move || async move {
        while remaining_ms(timeout_ms, shown_at, *now.peek()) > 0 {
            platform::sleep_ms(1000).await;
            now.set(platform::now_ms());
        }
    });
    let remaining = remaining_ms(timeout_ms, shown_at, now());
    rsx! {
        span { class: "countdown", role: "timer", "aria-label": "Time left", "data-testid": "countdown",
            "data-expired": remaining == 0,
            "{format_countdown(remaining)}"
        }
    }
}
