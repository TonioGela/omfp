//! The thread list (UI-020..022): grouping, filtering (archived threads only under the "Archived" filter, UI-026),
//! state badges, workspace chips, the attention shortcut, and the theme override (UI-091).

use dioxus::prelude::*;

use super::Ctx;
use crate::components::{StatusBadge, WorkspaceChipView};
use crate::local::Theme;
use crate::vm::new_thread::NewThreadForm;
use crate::vm::threads::{
    Dimension, GroupKey, Grouping, ThreadStatus, filter_options, group_threads, is_unread,
    next_needing_attention, workspace_chips,
};

/// Separator of `dimension` and key in the filter selector's values.
const KEY_SEPARATOR: char = ':';
/// The filter selector's value listing archived threads (UI-026); it has no separator, so it is never a key.
const ARCHIVED: &str = "archived";

#[component]
pub(crate) fn Sidebar() -> Element {
    let ctx = Ctx::get();
    let input = ctx.input.read();
    let state = ctx.ui.read();
    let groups = group_threads(
        &input.threads,
        &input.workspaces,
        &input.repositories,
        state.grouping,
        &state.filter,
    );
    let options = filter_options(&input.threads, &input.workspaces, &input.repositories);
    let waiting = input
        .threads
        .iter()
        .filter(|thread| thread.archived_at.is_none() && thread.needs_attention)
        .count();
    let grouped = state.grouping != Grouping::None;
    drop(state);
    drop(input);

    rsx! {
        nav { class: "sidebar", "aria-label": "Threads", "data-testid": "sidebar",
            SidebarActions { waiting }
            SidebarControls { options }
            for group in groups {
                section { key: "{group.key}", class: "thread-group", "data-testid": "thread-group", "data-group": "{group.key}",
                    if grouped {
                        h3 { class: "group-label", "{group.label}" }
                    }
                    ul { class: "thread-list",
                        for thread_id in group.thread_ids {
                            ThreadRow { key: "{thread_id}", thread_id }
                        }
                    }
                }
            }
            ThemePicker {}
        }
    }
}

/// New thread, and the "next thread needing attention" shortcut (UI-021).
#[component]
fn SidebarActions(waiting: usize) -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    rsx! {
        div { class: "sidebar-actions",
            button {
                r#type: "button",
                "data-testid": "new-thread",
                title: "New thread (Alt+N)",
                onclick: move |_| {
                    ui.write().new_thread = Some(NewThreadForm::default());
                },
                "+ thread"
            }
            button {
                r#type: "button",
                "data-testid": "next-attention",
                title: "Next thread needing attention (Alt+A)",
                disabled: waiting == 0,
                onclick: move |_| {
                    let selected = ui.peek().selected.clone();
                    if let Some(thread_id) = next_needing_attention(&ctx.input.peek().threads, selected.as_deref()) {
                        ctx.select_thread(&thread_id);
                    }
                },
                "Next waiting ({waiting})"
            }
        }
    }
}

/// Grouping, filter key (or archived threads) and free-text filter (UI-022, UI-026).
#[component]
fn SidebarControls(options: Vec<(Dimension, GroupKey)>) -> Element {
    let mut ui = Ctx::get().ui;
    let state = ui.read();
    let grouping = state.grouping.token();
    let filter_value = if state.filter.archived {
        ARCHIVED.to_owned()
    } else {
        state
            .filter
            .key
            .as_ref()
            .map(|(dimension, key)| format!("{dimension}{KEY_SEPARATOR}{key}"))
            .unwrap_or_default()
    };
    let filter_text = state.filter.text.clone();
    drop(state);
    rsx! {
        div { class: "sidebar-controls",
            label {
                "Group "
                select {
                    "data-testid": "grouping",
                    value: grouping,
                    onchange: move |event| ui.write().grouping = Grouping::from_token(&event.value()),
                    option { value: "none", "none" }
                    for dimension in Dimension::ALL {
                        option { value: dimension.token(), selected: dimension.token() == grouping, "{dimension}" }
                    }
                }
            }
            label {
                "Filter "
                select {
                    "data-testid": "filter-key",
                    value: "{filter_value}",
                    onchange: move |event| {
                        let value = event.value();
                        let mut state = ui.write();
                        state.filter.archived = value == ARCHIVED;
                        state.filter.key = value.split_once(KEY_SEPARATOR).and_then(|(dimension, key)| {
                            Some((Dimension::from_token(dimension)?, key.to_owned()))
                        });
                    },
                    option { value: "", "all" }
                    option { value: ARCHIVED, selected: filter_value == ARCHIVED, "Archived" }
                    for (dimension, key) in options {
                        option {
                            value: "{dimension}{KEY_SEPARATOR}{key.key}",
                            selected: format!("{dimension}{KEY_SEPARATOR}{}", key.key) == filter_value,
                            "{dimension}: {key.label}"
                        }
                    }
                }
            }
            input {
                r#type: "search",
                "data-testid": "filter-text",
                "aria-label": "Search threads",
                placeholder: "Search threads",
                value: "{filter_text}",
                oninput: move |event| ui.write().filter.text = event.value(),
            }
        }
    }
}

/// System, light or dark (UI-091).
#[component]
fn ThemePicker() -> Element {
    let mut ui = Ctx::get().ui;
    let theme = ui.read().theme.token();
    rsx! {
        div { class: "sidebar-footer",
            label {
                "Theme "
                select {
                    "data-testid": "theme",
                    value: theme,
                    onchange: move |event| ui.write().theme = Theme::from_token(&event.value()),
                    for choice in Theme::ALL {
                        option { value: choice.token(), selected: choice.token() == theme, "{choice.label()}" }
                    }
                }
            }
        }
    }
}

#[component]
fn ThreadRow(thread_id: String) -> Element {
    let ctx = Ctx::get();
    let input = ctx.input.read();
    let state = ctx.ui.read();
    let Some(thread) = input.threads.iter().find(|thread| thread.id == thread_id) else {
        return rsx! {};
    };
    let status = ThreadStatus::of(thread);
    let selected = state.selected.as_deref() == Some(thread_id.as_str());
    let unread = is_unread(thread, &state.seen, state.selected.as_deref());
    let chips = workspace_chips(
        &thread.workspace_ids,
        &input.workspaces,
        &input.repositories,
    );
    let title = thread.title.clone();
    drop(state);
    drop(input);
    let id = thread_id.clone();
    rsx! {
        li {
            button {
                r#type: "button",
                class: "thread-row",
                "data-testid": "thread-row",
                "data-thread-id": "{thread_id}",
                "data-status": status.token(),
                "aria-current": if selected { "true" },
                onclick: move |_| ctx.select_thread(&id),
                StatusBadge { status, testid: "thread-status" }
                span { class: "thread-title", "{title}" }
                if unread {
                    span { class: "unread", "data-testid": "unread", "aria-label": "unread", "•" }
                }
                if !chips.is_empty() {
                    span { class: "chips",
                        for chip in chips {
                            WorkspaceChipView { key: "{chip.workspace_id}", chip, link: false }
                        }
                    }
                }
            }
        }
    }
}
