//! Queue chips (UI-062) and the composer (UI-060, UI-061, UI-063, UI-064).

use dioxus::html::{FileData, HasFileData};
use dioxus::prelude::*;
use omfp_core::wire::ImageContent;

use super::Ctx;
use crate::intent::{Intent, QueueOp};
use crate::vm::composer::{
    ComposerKey, KeyAction, Modifiers, SendMode, Suggestion, complete, image_attachment, is_busy,
    key_action, slash_suggestions,
};
use crate::vm::queue::{PendingQueueOp, QueueChip, queue_chips};

#[component]
pub(crate) fn QueueChips(thread_id: String) -> Element {
    let ctx = Ctx::get();
    let input = ctx.input.read();
    let Some(open) = input.open.get(&thread_id) else {
        return rsx! {};
    };
    let chips = queue_chips(&open.state.queue, &thread_id, &ctx.ui.read().pending_queue);
    drop(input);
    if chips.is_empty() {
        return rsx! {};
    }
    rsx! {
        ul { class: "queue", "data-testid": "queue", "aria-label": "Queued messages",
            for (index, chip) in chips.into_iter().enumerate() {
                QueueChipView { key: "{index}", thread_id: thread_id.clone(), chip }
            }
        }
    }
}

/// One chip; its actions mark it pending until the daemon answers.
#[component]
fn QueueChipView(thread_id: String, chip: QueueChip) -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let pending = chip.pending.map(|op| match op {
        QueueOp::Remove => "remove",
        QueueOp::Promote => "promote",
    });
    let ops: Vec<QueueOp> = [QueueOp::Promote, QueueOp::Remove]
        .into_iter()
        .filter(|op| *op == QueueOp::Remove || chip.can_promote)
        .collect();
    rsx! {
        li {
            class: "queue-chip",
            "data-testid": "queue-chip",
            "data-queue": chip.queue_token(),
            "data-pending": pending,
            "aria-busy": chip.pending.is_some(),
            span { class: "queue-kind", "{chip.queue_token()}" }
            span { class: "queue-text", "{chip.text}" }
            for op in ops {
                button {
                    r#type: "button",
                    "data-testid": if op == QueueOp::Remove { "queue-remove" } else { "queue-promote" },
                    disabled: chip.pending.is_some(),
                    onclick: {
                        let thread_id = thread_id.clone();
                        let message = chip.text.clone();
                        let queue = chip.queue;
                        move |_| {
                            ui.write().pending_queue.push(PendingQueueOp {
                                thread_id: thread_id.clone(),
                                op,
                                message: message.clone(),
                            });
                            ctx.send(Intent::Queue {
                                thread_id: thread_id.clone(),
                                op,
                                message: message.clone(),
                                queue,
                            });
                        }
                    },
                    if op == QueueOp::Remove { "Remove" } else { "Steer now" }
                }
            }
        }
    }
}

fn composer_key(key: &Key) -> ComposerKey {
    match key {
        Key::Enter => ComposerKey::Enter,
        Key::Escape => ComposerKey::Escape,
        Key::Tab => ComposerKey::Tab,
        _ => ComposerKey::Other,
    }
}

fn modifiers(event: &KeyboardEvent) -> Modifiers {
    let held = event.modifiers();
    Modifiers {
        shift: held.shift(),
        alt: held.alt(),
        ctrl: held.ctrl(),
        meta: held.meta(),
    }
}

/// Sends the draft of `thread_id` with `mode` and clears it; blank drafts are not sent.
fn send_draft(ctx: Ctx, thread_id: &str, mode: SendMode) {
    let mut ui = ctx.ui;
    let draft = ui.peek().draft(thread_id);
    if draft.is_blank() {
        return;
    }
    ui.write().drafts.remove(thread_id);
    ctx.send(Intent::Prompt {
        thread_id: thread_id.to_owned(),
        text: draft.text.trim().to_owned(),
        images: draft.images,
        behavior: mode.behavior(),
    });
}

/// Replaces the draft text of `thread_id`.
fn set_draft_text(ctx: Ctx, thread_id: &str, text: String) {
    let mut ui = ctx.ui;
    ui.write()
        .drafts
        .entry(thread_id.to_owned())
        .or_default()
        .text = text;
}

/// Appends an image to the draft of `thread_id`.
fn attach(ctx: Ctx, thread_id: &str, image: ImageContent) {
    let mut ui = ctx.ui;
    ui.write()
        .drafts
        .entry(thread_id.to_owned())
        .or_default()
        .images
        .push(image);
}

/// Attaches the dropped image files (UI-060); other files are ignored.
fn attach_dropped(ctx: Ctx, thread_id: String, files: Vec<FileData>) {
    spawn(async move {
        for file in files {
            let mime = file.content_type().unwrap_or_default();
            if !mime.starts_with("image/") {
                continue;
            }
            if let Ok(bytes) = file.read_bytes().await
                && let Some(image) = image_attachment(&mime, &bytes)
            {
                attach(ctx, &thread_id, image);
            }
        }
    });
}

/// Attaches pasted image files (UI-060); a paste without images stays a text paste.
fn attach_pasted(ctx: Ctx, thread_id: String, event: &ClipboardEvent) {
    #[cfg(target_arch = "wasm32")]
    {
        let files = crate::platform::pasted_images(&event.data());
        if files.is_empty() {
            return;
        }
        event.prevent_default();
        spawn(async move {
            for file in files {
                if let Some((mime, bytes)) = crate::platform::read_file(file).await
                    && let Some(image) = image_attachment(&mime, &bytes)
                {
                    attach(ctx, &thread_id, image);
                }
            }
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (ctx, thread_id, event);
}

#[component]
pub(crate) fn Composer(thread_id: String) -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let input = ctx.input.read();
    let Some(open) = input.open.get(&thread_id) else {
        return rsx! {};
    };
    let busy = is_busy(&open.state);
    let state = ui.read();
    let draft = state.draft(&thread_id);
    let suggestions = slash_suggestions(&draft.text, &open.state.commands);
    let mode = SendMode::of(busy, state.steer_modifier);
    drop(state);
    drop(input);

    let id = thread_id.clone();
    let first_suggestion = suggestions.first().cloned();
    let on_keydown = move |event: KeyboardEvent| {
        let held = modifiers(&event);
        if ui.peek().steer_modifier != held.steer() {
            ui.write().steer_modifier = held.steer();
        }
        match key_action(
            composer_key(&event.key()),
            held,
            busy,
            first_suggestion.is_some(),
        ) {
            KeyAction::None => {}
            KeyAction::Send(mode) => {
                event.prevent_default();
                send_draft(ctx, &id, mode);
            }
            KeyAction::Abort => {
                event.prevent_default();
                ctx.send(Intent::Abort {
                    thread_id: id.clone(),
                });
            }
            KeyAction::Complete => {
                event.prevent_default();
                if let Some(suggestion) = &first_suggestion {
                    set_draft_text(ctx, &id, complete(suggestion));
                }
            }
        }
    };
    let on_keyup = move |event: KeyboardEvent| {
        let steer = modifiers(&event).steer();
        if ui.peek().steer_modifier != steer {
            ui.write().steer_modifier = steer;
        }
    };
    let (submit_id, input_id, drop_id, paste_id) = (
        thread_id.clone(),
        thread_id.clone(),
        thread_id.clone(),
        thread_id.clone(),
    );

    rsx! {
        form {
            class: "composer",
            "data-testid": "composer",
            "data-busy": busy,
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                send_draft(ctx, &submit_id, mode);
            },
            ondragover: move |event: DragEvent| event.prevent_default(),
            ondrop: move |event: DragEvent| {
                event.prevent_default();
                attach_dropped(ctx, drop_id.clone(), event.files());
            },
            if !draft.images.is_empty() {
                Attachments { thread_id: thread_id.clone(), images: draft.images.clone() }
            }
            if !suggestions.is_empty() {
                SlashSuggestions { thread_id: thread_id.clone(), suggestions }
            }
            textarea {
                class: "composer-input",
                "data-testid": "composer-input",
                "aria-label": "Message",
                "aria-keyshortcuts": "Enter Shift+Enter Alt+Enter Escape Tab",
                placeholder: if busy { "Queue a follow-up (Alt+Enter steers, Esc aborts)" } else { "Message (/ for commands)" },
                rows: "3",
                value: "{draft.text}",
                oninput: move |event| set_draft_text(ctx, &input_id, event.value()),
                onkeydown: on_keydown,
                onkeyup: on_keyup,
                onpaste: move |event| attach_pasted(ctx, paste_id.clone(), &event),
            }
            ComposerActions { thread_id, mode, busy }
        }
    }
}

/// Send (labelled with what it does, UI-061), the other busy mode, and abort.
#[component]
fn ComposerActions(thread_id: String, mode: SendMode, busy: bool) -> Element {
    let ctx = Ctx::get();
    let (alternative_id, abort_id) = (thread_id.clone(), thread_id);
    rsx! {
        div { class: "composer-actions",
            button { r#type: "submit", class: "primary", "data-testid": "send", "data-mode": mode.token(), "{mode.label()}" }
            if let Some(alternative) = mode.alternative() {
                button {
                    r#type: "button",
                    "data-testid": "send-alternative",
                    "data-mode": alternative.token(),
                    onclick: move |_| send_draft(ctx, &alternative_id, alternative),
                    "{alternative.label()}"
                }
            }
            if busy {
                button {
                    r#type: "button",
                    class: "danger",
                    "data-testid": "abort",
                    onclick: move |_| ctx.send(Intent::Abort { thread_id: abort_id.clone() }),
                    "Abort (Esc)"
                }
            }
        }
    }
}

/// Thumbnails of the attached images, each removable.
#[component]
fn Attachments(thread_id: String, images: Vec<ImageContent>) -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    rsx! {
        ul { class: "attachments", "aria-label": "Attached images",
            for (index, image) in images.iter().enumerate() {
                li { key: "{index}", class: "attachment", "data-testid": "attachment",
                    img {
                        src: "data:{image.mime_type.clone().unwrap_or_default()};base64,{image.data.clone().unwrap_or_default()}",
                        alt: "Attached image {index + 1}",
                    }
                    button {
                        r#type: "button",
                        "aria-label": "Remove image {index + 1}",
                        onclick: {
                            let id = thread_id.clone();
                            move |_| {
                                if let Some(draft) = ui.write().drafts.get_mut(&id)
                                    && index < draft.images.len()
                                {
                                    draft.images.remove(index);
                                }
                            }
                        },
                        "×"
                    }
                }
            }
        }
    }
}

/// `/` command completions (UI-060); Tab or a click completes.
#[component]
fn SlashSuggestions(thread_id: String, suggestions: Vec<Suggestion>) -> Element {
    let ctx = Ctx::get();
    rsx! {
        ul { class: "suggestions", role: "listbox", "aria-label": "Commands", "data-testid": "slash-suggestions",
            for (index, suggestion) in suggestions.into_iter().enumerate() {
                li { key: "{suggestion.name}", role: "option", "aria-selected": index == 0,
                    button {
                        r#type: "button",
                        "data-testid": "slash-suggestion",
                        onclick: {
                            let id = thread_id.clone();
                            let completed = complete(&suggestion);
                            move |_| set_draft_text(ctx, &id, completed.clone())
                        },
                        span { class: "command-name", "/{suggestion.name}" }
                        if let Some(hint) = suggestion.hint.as_ref() {
                            span { class: "command-hint", " {hint}" }
                        }
                        if let Some(description) = suggestion.description.as_ref() {
                            span { class: "command-description", " — {description}" }
                        }
                    }
                }
            }
        }
    }
}
