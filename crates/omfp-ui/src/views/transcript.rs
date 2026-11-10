//! The virtualized transcript (UI-042, spike S-06). The scroll container reports its viewport on scroll, each rendered
//! item reports its height through a resize observer, and the content reports growth: while pinned, growth scrolls the
//! container to the bottom. Pending interactions sit after the list, inside the same scroll container.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use dioxus::html::geometry::PixelsVector2D;
use dioxus::prelude::*;
use omfp_core::ThreadState;

use super::Ctx;
use super::interactions::Interactions;
use crate::components::{MessageBlocks, NoticeRowView};
use crate::vm::notice::{NoticeRow, notice_row};
use crate::vm::transcript::{ItemKind, MessageView, items, message_view, streaming_view};
use crate::vm::virtual_list::{ListMetrics, Window};

/// What one rendered item shows.
#[derive(Debug, Clone, PartialEq)]
enum RowView {
    Message(MessageView),
    Streaming(MessageView),
    Notice(NoticeRow),
}

/// Far enough down to reach the bottom of any transcript.
const BOTTOM: f64 = 1.0e9;

/// A CSS pixel length, without a negative zero.
fn px(length: f64) -> String {
    format!("{:.0}px", length.max(0.0) + 0.0)
}

/// The window over `state`'s items and the views of the rendered ones.
fn visible_rows(
    state: &ThreadState,
    metrics: Option<&ListMetrics>,
) -> (Window, Vec<(String, RowView)>) {
    let items = items(state);
    let keys: Vec<String> = items.iter().map(|item| item.key.clone()).collect();
    let window = metrics.map_or_else(
        || ListMetrics::default().window(&keys),
        |known| known.window(&keys),
    );
    let rows = items[window.start..window.end]
        .iter()
        .filter_map(|item| {
            let row = match item.kind {
                ItemKind::Message(index) => RowView::Message(message_view(state, index)),
                ItemKind::Notice(index) => RowView::Notice(notice_row(&state.notices[index].frame)),
                ItemKind::Streaming => RowView::Streaming(streaming_view(state)?),
            };
            Some((item.key.clone(), row))
        })
        .collect();
    (window, rows)
}

/// Changes the layout metrics of `thread_id`.
fn update_metrics(
    mut metrics: Signal<BTreeMap<String, ListMetrics>>,
    thread_id: &str,
    change: impl FnOnce(&mut ListMetrics),
) {
    change(metrics.write().entry(thread_id.to_owned()).or_default());
}

/// Scrolls `container` to `top`.
fn scroll_to(container: Rc<MountedData>, top: f64) {
    spawn(async move {
        // A failed scroll leaves the view where it is; the next growth retries.
        let _ = container
            .scroll(PixelsVector2D::new(0.0, top), ScrollBehavior::Instant)
            .await;
    });
}

/// The transcript of `thread_id`, remounted for each thread: a fresh scroll container never shows the previous
/// thread's scroll position, and mounting restores this thread's own.
#[component]
pub(crate) fn Transcript(thread_id: String) -> Element {
    rsx! {
        ThreadTranscript { key: "{thread_id}", thread_id }
    }
}

#[component]
fn ThreadTranscript(thread_id: String) -> Element {
    let ctx = Ctx::get();
    let metrics = ctx.metrics;
    let mut container: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    // Set by the requests' keydown handler, which runs before the bubbling event reaches the list.
    let from_control = use_hook(|| Rc::new(Cell::new(false)));

    let input = ctx.input.read();
    let Some(open) = input.open.get(&thread_id) else {
        return rsx! {};
    };
    let known = metrics.read();
    let (window, rows) = visible_rows(&open.state, known.get(&thread_id));
    let pinned = known.get(&thread_id).is_none_or(|known| known.pinned);
    drop(known);
    drop(input);

    let id = thread_id.clone();
    let on_scroll = move |event: ScrollEvent| {
        let (top, height, client) = (
            event.scroll_top(),
            f64::from(event.scroll_height()),
            f64::from(event.client_height()),
        );
        update_metrics(metrics, &id, |metrics| {
            metrics.on_scroll(top, height, client);
        });
    };
    let id = thread_id.clone();
    let on_wheel = move |event: WheelEvent| {
        if event.delta().strip_units().y < 0.0 && pinned {
            update_metrics(metrics, &id, ListMetrics::scrolled_up_by_user);
        }
    };
    let id = thread_id.clone();
    let typed_in_control = from_control.clone();
    let on_keydown = move |event: KeyboardEvent| {
        let scroll_up_key = matches!(event.key(), Key::ArrowUp | Key::PageUp | Key::Home);
        let in_control = typed_in_control.replace(false);
        if pinned && scroll_up_key {
            update_metrics(metrics, &id, |metrics| {
                metrics.on_key(scroll_up_key, in_control);
            });
        }
    };
    let mount_id = thread_id.clone();
    let on_mounted = move |event: MountedEvent| {
        let data = event.data();
        container.set(Some(data.clone()));
        let restored = metrics
            .peek()
            .get(&mount_id)
            .and_then(ListMetrics::mount_scroll_top);
        scroll_to(data, restored.unwrap_or(BOTTOM));
    };
    let (down_id, up_id) = (thread_id.clone(), thread_id.clone());
    let (touch_id, untouch_id) = (thread_id.clone(), thread_id.clone());
    let resize_id = thread_id.clone();
    let on_content_resize = move |_| {
        let pinned = metrics
            .peek()
            .get(&resize_id)
            .is_none_or(|metrics| metrics.pinned);
        if pinned && let Some(container) = container.peek().clone() {
            scroll_to(container, BOTTOM);
        }
    };

    rsx! {
        div {
            class: "transcript",
            "data-testid": "transcript",
            "data-thread-id": "{thread_id}",
            "data-pinned": pinned,
            role: "log",
            "aria-label": "Transcript",
            tabindex: "0",
            onmounted: on_mounted,
            onscroll: on_scroll,
            onwheel: on_wheel,
            onkeydown: on_keydown,
            // Dragging the scrollbar or touching the list: scroll events now follow the user.
            onpointerdown: move |_| update_metrics(metrics, &down_id, |metrics| metrics.dragging = true),
            onpointerup: move |_| update_metrics(metrics, &up_id, |metrics| metrics.dragging = false),
            ontouchstart: move |_| update_metrics(metrics, &touch_id, |metrics| metrics.dragging = true),
            ontouchend: move |_| update_metrics(metrics, &untouch_id, |metrics| metrics.dragging = false),
            div { class: "transcript-content", onresize: on_content_resize,
                div { class: "spacer", "data-testid": "spacer-top", style: "height: {px(window.pad_top)}" }
                for (key, row) in rows {
                    TranscriptRow { key: "{key}", thread_id: thread_id.clone(), item_key: key.clone(), row }
                }
                div { class: "spacer", "data-testid": "spacer-bottom", style: "height: {px(window.pad_bottom)}" }
                Interactions { thread_id: thread_id.clone(), onkeydown: move |_| from_control.set(true) }
            }
        }
    }
}

/// One item, measured whenever its size changes.
#[component]
fn TranscriptRow(thread_id: String, item_key: String, row: RowView) -> Element {
    let ctx = Ctx::get();
    let mut metrics = ctx.metrics;
    let card_key = format!("{thread_id}/{item_key}");
    let measured_key = item_key.clone();
    let on_resize = move |event: ResizeEvent| {
        let Ok(size) = event.get_border_box_size() else {
            return;
        };
        let changed = metrics.peek().get(&thread_id).is_none_or(|known| {
            known
                .heights
                .get(&measured_key)
                .is_none_or(|height| (height - size.height).abs() >= 0.5)
        });
        if changed {
            metrics
                .write()
                .entry(thread_id.clone())
                .or_default()
                .record_height(&measured_key, size.height);
        }
    };
    rsx! {
        div { class: "transcript-item", "data-testid": "transcript-item", "data-key": "{item_key}", onresize: on_resize,
            match row {
                RowView::Message(view) => rsx! {
                    MessageBlocks { card_key, view, streaming: false }
                },
                RowView::Streaming(view) => rsx! {
                    MessageBlocks { card_key, view, streaming: true }
                },
                RowView::Notice(row) => rsx! {
                    NoticeRowView { card_key, row }
                },
            }
        }
    }
}
