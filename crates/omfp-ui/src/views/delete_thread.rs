//! The delete confirmation (UI-026): deleting removes the thread's history, never its workspaces. Like every dialog it
//! traps focus (UI-092): the shell behind it is inert and focus sentinels at both ends wrap Tab around; Cancel has the
//! focus when it opens.

use std::rc::Rc;

use dioxus::prelude::*;

use super::Ctx;
use super::new_thread::focus;
use crate::intent::Intent;

#[component]
pub(crate) fn DeleteThreadDialog() -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let first: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let last: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let Some(thread_id) = ui.read().delete_thread.clone() else {
        return rsx! {};
    };
    let title = ctx
        .input
        .read()
        .threads
        .iter()
        .find(|thread| thread.id == thread_id)
        .map_or_else(|| thread_id.clone(), |thread| thread.title.clone());
    let mut close = move || ui.write().delete_thread = None;
    let on_delete = move |_| {
        ui.write().drop_thread(&thread_id);
        ctx.send(Intent::DeleteThread {
            thread_id: thread_id.clone(),
        });
    };

    rsx! {
        div { class: "dialog-backdrop",
            div {
                class: "dialog",
                role: "dialog",
                "aria-modal": "true",
                "aria-labelledby": "delete-thread-heading",
                "aria-describedby": "delete-thread-message",
                "data-testid": "delete-thread-dialog",
                onkeydown: move |event: KeyboardEvent| {
                    if event.key() == Key::Escape {
                        event.prevent_default();
                        close();
                    }
                },
                div { tabindex: "0", "data-testid": "focus-start", onfocus: move |_| focus(last) }
                h2 { id: "delete-thread-heading", "Delete \u{201c}{title}\u{201d}?" }
                p { id: "delete-thread-message", "data-testid": "delete-thread-message",
                    "The thread's history (transcript, session and log) is removed for good. Its workspaces are kept."
                }
                div { class: "dialog-actions",
                    button {
                        r#type: "button",
                        class: "danger",
                        "data-testid": "confirm-delete-thread",
                        onmounted: move |event| {
                            let mut first = first;
                            first.set(Some(event.data()));
                        },
                        onclick: on_delete,
                        "Delete"
                    }
                    button {
                        r#type: "button",
                        "data-testid": "cancel-delete-thread",
                        onmounted: move |event| {
                            let mut last = last;
                            last.set(Some(event.data()));
                            focus(last);
                        },
                        onclick: move |_| close(),
                        "Cancel"
                    }
                }
                div { tabindex: "0", "data-testid": "focus-end", onfocus: move |_| focus(first) }
            }
        }
    }
}
