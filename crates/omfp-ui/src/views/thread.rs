//! The main area: the selected thread (header, transcript, interactions, queue, composer) and main-placed panels.

use dioxus::prelude::*;

use super::Ctx;
use super::composer::{Composer, QueueChips};
use super::header::Header;
use super::panels::PanelFrame;
use super::transcript::Transcript;
use crate::panels::Placement;

#[component]
pub(crate) fn MainArea() -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let state = ui.read();
    let tabs: Vec<(String, String)> = state
        .panels
        .in_placement(Placement::Main)
        .into_iter()
        .map(|panel| (panel.id.clone(), panel.title.clone()))
        .collect();
    let active = state
        .main_tab
        .clone()
        .filter(|id| tabs.iter().any(|(tab, _)| tab == id));
    drop(state);
    rsx! {
        main { class: "main", "data-testid": "main",
            if !tabs.is_empty() {
                div { class: "tabs", role: "tablist", "aria-label": "Main area",
                    button {
                        r#type: "button",
                        role: "tab",
                        "aria-selected": active.is_none(),
                        "data-testid": "main-tab",
                        onclick: move |_| ui.write().main_tab = None,
                        "Thread"
                    }
                    for (id, title) in tabs {
                        button {
                            key: "{id}",
                            r#type: "button",
                            role: "tab",
                            "aria-selected": active.as_deref() == Some(id.as_str()),
                            "data-testid": "main-tab",
                            onclick: {
                                let id = id.clone();
                                move |_| ui.write().main_tab = Some(id.clone())
                            },
                            "{title}"
                        }
                    }
                }
            }
            match active {
                Some(id) => rsx! {
                    PanelFrame { panel_id: id }
                },
                None => rsx! {
                    ThreadPane {}
                },
            }
        }
    }
}

#[component]
fn ThreadPane() -> Element {
    let ctx = Ctx::get();
    let input = ctx.input.read();
    let selected = ctx.ui.read().selected.clone();
    let Some(thread_id) = selected.filter(|id| input.threads.iter().any(|thread| thread.id == *id))
    else {
        return rsx! {
            div { class: "empty", "data-testid": "no-thread", "Select a thread, or create one with “+ thread”." }
        };
    };
    let open = input.open.contains_key(&thread_id);
    drop(input);
    rsx! {
        section { class: "thread", "data-testid": "thread", "data-thread-id": "{thread_id}", "aria-label": "Thread",
            Header { thread_id: thread_id.clone() }
            if open {
                Transcript { thread_id: thread_id.clone() }
                QueueChips { thread_id: thread_id.clone() }
                Composer { thread_id: thread_id.clone() }
            } else {
                div { class: "empty", "data-testid": "thread-loading", "Loading the thread…" }
            }
        }
    }
}
