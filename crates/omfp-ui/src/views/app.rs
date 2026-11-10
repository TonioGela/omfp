//! The root component: provides the shared context, applies outcomes, and lays out the shell.

use std::collections::BTreeMap;

use dioxus::prelude::*;

use super::Ctx;
use super::new_thread::NewThreadDialog;
use super::panels::{BottomPanels, FloatingPanels, SidePanels};
use super::sidebar::Sidebar;
use super::thread::MainArea;
use crate::components::{ConnectionBanner, LiveRegion};
use crate::input::ViewInput;
use crate::intent::Intent;
use crate::local::{Theme, UiState};
use crate::vm::connection::announcement;
use crate::vm::new_thread::NewThreadForm;
use crate::vm::threads::next_needing_attention;

const STYLE: Asset = asset!("/assets/omfp.css");

/// The app. `input` is the daemon's data, kept by the host; every user action arrives at `on_intent`; `initial` seeds
/// the window-local state (selection, drafts, …) on the first render.
#[component]
pub fn App(
    input: ReadSignal<ViewInput>,
    on_intent: EventHandler<Intent>,
    #[props(default)] initial: UiState,
) -> Element {
    let ui = use_signal(|| initial.clone());
    let metrics = use_signal(BTreeMap::new);
    let ctx = use_context_provider(|| Ctx {
        input,
        ui,
        metrics,
        dispatch: on_intent,
    });
    use_input_effects(ctx);

    let state = ui.read();
    let view = input.read();
    let dialog_open = state.new_thread.is_some();
    let theme = match state.theme {
        Theme::System => None,
        theme => Some(theme.token()),
    };
    let status = announcement(&view.connection, &view.threads, state.selected.as_deref());
    let errors = state.errors.clone();
    let connection = view.connection.clone();
    drop(view);
    drop(state);

    rsx! {
        document::Stylesheet { href: STYLE }
        div {
            class: "app",
            "data-testid": "app",
            "data-theme": theme,
            // Shortcuts listen here: the app takes the focus when the page loads and whenever a click lands outside
            // any control, so keys never go to `<body>` past the handler.
            tabindex: "-1",
            onmounted: move |event| async move {
                // Best effort: there may be no focusable document (server-side rendering).
                let _ = event.data().set_focus(true).await;
            },
            onkeydown: move |event| global_shortcut(ctx, &event),
            ConnectionBanner { connection }
            // Dioxus web writes `inert="false"` for a false bool, and any `inert` value disables the subtree.
            div { class: "shell", "data-testid": "shell", inert: if dialog_open { "true" },
                Sidebar {}
                MainArea {}
                SidePanels {}
                BottomPanels {}
            }
            FloatingPanels {}
            if !errors.is_empty() {
                Errors { errors }
            }
            LiveRegion { text: status }
            if dialog_open {
                NewThreadDialog {}
            }
        }
    }
}

/// Re-opens the thread selected in the initial state once mounted, and keeps unread baselines and intent outcomes in
/// step with the input.
fn use_input_effects(ctx: Ctx) {
    let (input, mut ui) = (ctx.input, ctx.ui);
    use_effect(move || {
        if let Some(thread_id) = ui.peek().selected.clone() {
            ctx.send(Intent::SelectThread { thread_id });
        }
    });
    use_effect(move || {
        let input = input.read();
        let (updates, fresh) = {
            let state = ui.peek();
            (
                state.seen_updates(&input.threads),
                input
                    .outcomes
                    .last()
                    .is_some_and(|entry| state.is_fresh_outcome(entry.seq)),
            )
        };
        if updates.is_empty() && !fresh {
            return;
        }
        let follow_ups = {
            let mut state = ui.write();
            state.seen.extend(updates);
            state.apply_outcomes(&input.outcomes)
        };
        for intent in follow_ups {
            ctx.send(intent);
        }
    });
}

/// Alt+A: next thread needing attention (UI-021); Alt+N: new thread.
fn global_shortcut(ctx: Ctx, event: &KeyboardEvent) {
    let mut ui = ctx.ui;
    let modifiers = event.modifiers();
    if !modifiers.alt() || modifiers.ctrl() || modifiers.meta() {
        return;
    }
    match event.code() {
        Code::KeyA => {
            event.prevent_default();
            let selected = ui.peek().selected.clone();
            let next = next_needing_attention(&ctx.input.peek().threads, selected.as_deref());
            if let Some(thread_id) = next {
                ctx.select_thread(&thread_id);
            }
        }
        Code::KeyN => {
            event.prevent_default();
            ui.write()
                .new_thread
                .get_or_insert_with(NewThreadForm::default);
        }
        _ => {}
    }
}

/// Errors to dismiss.
#[component]
fn Errors(errors: Vec<String>) -> Element {
    let mut ui = Ctx::get().ui;
    rsx! {
        ul { class: "toasts", role: "alert", "data-testid": "errors",
            for (index, error) in errors.into_iter().enumerate() {
                li { key: "{index}", class: "toast", "data-testid": "error",
                    span { "{error}" }
                    button {
                        r#type: "button",
                        "aria-label": "Dismiss",
                        onclick: move |_| {
                            let mut state = ui.write();
                            if index < state.errors.len() {
                                state.errors.remove(index);
                            }
                        },
                        "×"
                    }
                }
            }
        }
    }
}
