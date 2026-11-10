//! The thread header (UI-030): title and tags (editable), workspace and repository chips, model and thinking
//! selectors, context usage, session cost, process state with stop/restart, and archive/unarchive/delete (UI-026).

use dioxus::prelude::*;
use omfp_core::wire::ThinkingLevel;

use super::Ctx;
use crate::components::{StatusBadge, WorkspaceChipView};
use crate::intent::Intent;
use crate::local::HeaderEdit;
use crate::vm::header::{HeaderView, header_view, parse_tags, split_model};
use crate::vm::threads::{ThreadStatus, WorkspaceChip};

#[component]
pub(crate) fn Header(thread_id: String) -> Element {
    let ctx = Ctx::get();
    let input = ctx.input.read();
    let Some(summary) = input.threads.iter().find(|thread| thread.id == thread_id) else {
        return rsx! {};
    };
    let open = input.open.get(&thread_id);
    let view = header_view(
        summary,
        open.map(|open| &open.state),
        open.map(|open| &open.details),
        &input.workspaces,
    );
    drop(input);
    rsx! {
        header { class: "thread-header", "data-testid": "thread-header",
            HeaderTitle { thread_id: thread_id.clone(), view: view.clone() }
            HeaderWorkspaces { chips: view.chips.clone() }
            HeaderSession { thread_id: thread_id.clone(), view: view.clone() }
            HeaderProcess { thread_id: thread_id.clone(), view: view.clone() }
            HeaderLifecycle { thread_id, archived: view.archived }
        }
    }
}

/// Title and tags, or their editor.
#[component]
fn HeaderTitle(thread_id: String, view: HeaderView) -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let editing = ui.read().header_edit.clone();
    let Some(edit) = editing else {
        let tags = view.tags.join(", ");
        let title = view.title.clone();
        return rsx! {
            div { class: "header-title",
                h1 { "data-testid": "thread-title", "{view.title}" }
                ul { class: "tags", "aria-label": "Tags",
                    for tag in view.tags.iter() {
                        li { key: "{tag}", class: "tag", "data-testid": "thread-tag", "{tag}" }
                    }
                }
                button {
                    r#type: "button",
                    "data-testid": "edit-header",
                    onclick: move |_| {
                        ui.write().header_edit = Some(HeaderEdit { title: title.clone(), tags: tags.clone() });
                    },
                    "Edit"
                }
            }
        };
    };
    let on_save = move |event: FormEvent| {
        event.prevent_default();
        let Some(edit) = ui.write().header_edit.take() else {
            return;
        };
        let title = edit.title.trim().to_owned();
        ctx.send(Intent::UpdateThread {
            thread_id: thread_id.clone(),
            title: (!title.is_empty()).then_some(title),
            tags: Some(parse_tags(&edit.tags)),
        });
    };
    rsx! {
        div { class: "header-title",
            form { class: "header-edit", "data-testid": "header-edit", onsubmit: on_save,
                input {
                    "data-testid": "title-input",
                    "aria-label": "Title",
                    value: "{edit.title}",
                    oninput: move |event| {
                        if let Some(edit) = ui.write().header_edit.as_mut() {
                            edit.title = event.value();
                        }
                    },
                }
                input {
                    "data-testid": "tags-input",
                    "aria-label": "Tags, comma-separated",
                    value: "{edit.tags}",
                    oninput: move |event| {
                        if let Some(edit) = ui.write().header_edit.as_mut() {
                            edit.tags = event.value();
                        }
                    },
                }
                button { r#type: "submit", "data-testid": "header-save", "Save" }
                button { r#type: "button", onclick: move |_| ui.write().header_edit = None, "Cancel" }
            }
        }
    }
}

/// The workspace chip (the cwd), then one chip per repository of the workspace.
#[component]
fn HeaderWorkspaces(chips: Vec<WorkspaceChip>) -> Element {
    rsx! {
        div { class: "header-workspaces",
            for chip in chips {
                WorkspaceChipView { key: "{chip.key}", chip }
            }
        }
    }
}

/// Model and thinking selectors, context usage and session cost.
#[component]
fn HeaderSession(thread_id: String, view: HeaderView) -> Element {
    let ctx = Ctx::get();
    let thinking_value = view.thinking.map(ThinkingLevel::as_str).unwrap_or_default();
    let model_value = view.model.clone().unwrap_or_default();
    let id = thread_id.clone();
    let on_model = move |event: FormEvent| {
        if let Some((provider, model_id)) = split_model(&event.value()) {
            ctx.send(Intent::SetModel {
                thread_id: id.clone(),
                provider,
                model_id,
            });
        }
    };
    let levels = view.thinking_levels.clone();
    let on_thinking = move |event: FormEvent| {
        let value = event.value();
        if let Some(level) = levels.iter().find(|level| level.as_str() == value) {
            ctx.send(Intent::SetThinking {
                thread_id: thread_id.clone(),
                level: *level,
            });
        }
    };
    rsx! {
        div { class: "header-session",
            select { "data-testid": "model-select", "aria-label": "Model", value: "{model_value}", onchange: on_model,
                if view.model.is_none() {
                    option { value: "", "Model…" }
                }
                for model in view.models.iter() {
                    option { key: "{model.value}", value: "{model.value}", selected: model.value == model_value, "{model.label}" }
                }
            }
            select { "data-testid": "thinking-select", "aria-label": "Thinking level", value: thinking_value, onchange: on_thinking,
                if view.thinking.is_none() {
                    option { value: "", "Thinking…" }
                }
                for level in view.thinking_levels.iter() {
                    option { key: "{level.as_str()}", value: level.as_str(), selected: level.as_str() == thinking_value, "{level.as_str()}" }
                }
            }
            if let Some(context) = view.context.as_ref() {
                span { class: "meter", "data-testid": "context-usage", title: "Context window usage", "{context}" }
            }
            if let Some(cost) = view.cost.as_ref() {
                span { class: "meter", "data-testid": "session-cost", title: "Session cost", "{cost}" }
            }
        }
    }
}

/// Process state, failure reason, stop/restart.
#[component]
fn HeaderProcess(thread_id: String, view: HeaderView) -> Element {
    let ctx = Ctx::get();
    let stopped = matches!(view.status, ThreadStatus::Stopped | ThreadStatus::Failed);
    let on_process = move |_| {
        let thread_id = thread_id.clone();
        ctx.send(if stopped {
            Intent::Restart { thread_id }
        } else {
            Intent::Stop { thread_id }
        });
    };
    rsx! {
        div { class: "header-process",
            StatusBadge { status: view.status, testid: "thread-state" }
            span { class: "state-label", "{view.status.label()}" }
            if let Some(failure) = view.failure.as_ref() {
                span { class: "failure", "data-testid": "thread-failure", "{failure}" }
            }
            if view.can_stop || stopped {
                button { r#type: "button", "data-testid": "process-action", onclick: on_process,
                    if stopped { "Restart" } else { "Stop" }
                }
            }
        }
    }
}

/// Archive (unarchive when archived) and delete, which asks for confirmation first (UI-026).
#[component]
fn HeaderLifecycle(thread_id: String, archived: bool) -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let id = thread_id.clone();
    let on_archive = move |_| {
        let thread_id = id.clone();
        if archived {
            ctx.send(Intent::UnarchiveThread { thread_id });
        } else {
            ui.write().drop_thread(&thread_id);
            ctx.send(Intent::ArchiveThread { thread_id });
        }
    };
    rsx! {
        div { class: "header-lifecycle",
            button { r#type: "button", "data-testid": "archive-thread", onclick: on_archive,
                if archived { "Unarchive" } else { "Archive" }
            }
            button {
                r#type: "button",
                class: "danger",
                "data-testid": "delete-thread",
                onclick: move |_| ui.write().delete_thread = Some(thread_id.clone()),
                "Delete…"
            }
        }
    }
}
