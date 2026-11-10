//! Pending interactions inline at the bottom of the transcript (UI-070): approval selects, confirms, inputs, editors
//! and `ask` forms (UI-071), with their countdown (UI-073), and rows for requests closed elsewhere (UI-072).

use dioxus::prelude::*;
use omfp_core::wire::{
    AskUiRequest, ConfirmUiRequest, EditorUiRequest, ExtensionUiRequest, InputUiRequest,
    SelectUiRequest,
};
use omfp_protocol::InteractionResponse;

use super::Ctx;
use crate::components::Countdown;
use crate::intent::Intent;
use crate::local::InteractionKey;
use crate::platform;
use crate::vm::interaction::{
    AnswerDraft, AskForm, card_key, closed_rows, request_id, timeout_ms, title,
};

/// The open requests and closed rows of `thread_id`; `onkeydown` sees every key typed into their controls.
#[component]
pub(crate) fn Interactions(thread_id: String, onkeydown: EventHandler<KeyboardEvent>) -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let input = ctx.input.read();
    let Some(open) = input.open.get(&thread_id) else {
        return rsx! {};
    };
    let requests = open.state.interactions.clone();
    let state = ui.read();
    let key = |id: &str| (thread_id.clone(), id.to_owned());
    let closed = closed_rows(
        &open.closed,
        |id| state.answered_here.contains(&key(id)),
        |id| state.dismissed.contains(&key(id)),
    );
    drop(state);
    drop(input);
    if requests.is_empty() && closed.is_empty() {
        return rsx! {};
    }
    rsx! {
        section {
            class: "interactions",
            "data-testid": "interactions",
            "aria-label": "Requests",
            onkeydown: move |event| onkeydown.call(event),
            for (index, request) in requests.into_iter().enumerate() {
                InteractionCard {
                    key: "{card_key(&request, index)}",
                    thread_id: thread_id.clone(),
                    request,
                    first: index == 0,
                }
            }
            for row in closed {
                div { key: "closed-{row.id}", class: "interaction-closed", "data-testid": "interaction-closed",
                    strong { "{row.title}" }
                    span { " — {row.text}" }
                    button {
                        r#type: "button",
                        "aria-label": "Dismiss",
                        onclick: {
                            let key = (thread_id.clone(), row.id.clone());
                            move |_| {
                                ui.write().dismissed.insert(key.clone());
                            }
                        },
                        "×"
                    }
                }
            }
        }
    }
}

/// Moves keyboard focus to a freshly mounted control.
fn focus(event: &MountedEvent) {
    let data = event.data();
    spawn(async move {
        // Focus is best effort: an element removed meanwhile cannot take it.
        let _ = data.set_focus(true).await;
    });
}

#[component]
fn InteractionCard(thread_id: String, request: ExtensionUiRequest, first: bool) -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let id = request_id(&request).unwrap_or_default().to_owned();
    let key: InteractionKey = (thread_id.clone(), id.clone());
    let shown_at = use_hook({
        let key = key.clone();
        move || {
            ui.peek()
                .shown_at
                .get(&key)
                .copied()
                .unwrap_or_else(platform::now_ms)
        }
    });
    use_effect({
        let key = key.clone();
        move || {
            if !ui.peek().shown_at.contains_key(&key) {
                ui.write().shown_at.insert(key.clone(), shown_at);
            }
        }
    });
    let answered = ui.read().answered_here.contains(&key);
    let respond = {
        let key = key.clone();
        move |response: InteractionResponse| {
            ui.write().answered_here.insert(key.clone());
            ctx.send(Intent::Respond {
                thread_id: key.0.clone(),
                interaction_id: key.1.clone(),
                response,
            });
        }
    };
    let heading = title(&request);
    let timeout = timeout_ms(&request);
    let kind = match &request {
        ExtensionUiRequest::Select(_) => "select",
        ExtensionUiRequest::Confirm(_) => "confirm",
        ExtensionUiRequest::Input(_) => "input",
        ExtensionUiRequest::Editor(_) => "editor",
        _ => "ask",
    };
    rsx! {
        section {
            class: "interaction",
            "data-testid": "interaction",
            "data-kind": kind,
            "aria-label": "{heading}",
            "aria-busy": answered,
            div { class: "interaction-heading",
                h3 { class: "interaction-title", "{heading}" }
                if let Some(timeout) = timeout {
                    Countdown { timeout_ms: timeout, shown_at }
                }
            }
            match request {
                ExtensionUiRequest::Select(request) => rsx! {
                    SelectBody { request, disabled: answered, autofocus: first, on_respond: respond }
                },
                ExtensionUiRequest::Confirm(request) => rsx! {
                    ConfirmBody { request, disabled: answered, autofocus: first, on_respond: respond }
                },
                ExtensionUiRequest::Input(request) => rsx! {
                    InputBody { request, disabled: answered, autofocus: first, on_respond: respond }
                },
                ExtensionUiRequest::Editor(request) => rsx! {
                    EditorBody { request, disabled: answered, autofocus: first, on_respond: respond }
                },
                ExtensionUiRequest::Ask(request) => rsx! {
                    AskBody { form_key: key, request, disabled: answered, autofocus: first, on_respond: respond }
                },
                _ => rsx! {},
            }
        }
    }
}

#[component]
fn SelectBody(
    request: SelectUiRequest,
    disabled: bool,
    autofocus: bool,
    on_respond: EventHandler<InteractionResponse>,
) -> Element {
    let details = request.option_details.clone().unwrap_or_default();
    rsx! {
        div { class: "options", role: "group", "aria-label": "{request.title}",
            for (index, option) in request.options.iter().cloned().enumerate() {
                button {
                    key: "{index}",
                    r#type: "button",
                    "data-testid": "interaction-option",
                    disabled,
                    onmounted: move |event| {
                        if autofocus && index == 0 {
                            focus(&event);
                        }
                    },
                    onclick: {
                        let option = option.clone();
                        move |_| on_respond.call(InteractionResponse::Value(option.clone()))
                    },
                    "{option}"
                    if let Some(description) = details.get(index).and_then(|detail| detail.description.clone()) {
                        span { class: "option-description", "{description}" }
                    }
                }
            }
            button {
                r#type: "button",
                class: "secondary",
                "data-testid": "interaction-dismiss",
                disabled,
                onclick: move |_| on_respond.call(InteractionResponse::Cancelled),
                "Dismiss"
            }
        }
    }
}

#[component]
fn ConfirmBody(
    request: ConfirmUiRequest,
    disabled: bool,
    autofocus: bool,
    on_respond: EventHandler<InteractionResponse>,
) -> Element {
    rsx! {
        p { class: "interaction-message", "{request.message}" }
        div { class: "options",
            button {
                r#type: "button",
                "data-testid": "interaction-confirm",
                disabled,
                onmounted: move |event| {
                    if autofocus {
                        focus(&event);
                    }
                },
                onclick: move |_| on_respond.call(InteractionResponse::Confirmed(true)),
                "Yes"
            }
            button {
                r#type: "button",
                "data-testid": "interaction-deny",
                disabled,
                onclick: move |_| on_respond.call(InteractionResponse::Confirmed(false)),
                "No"
            }
        }
    }
}

#[component]
fn InputBody(
    request: InputUiRequest,
    disabled: bool,
    autofocus: bool,
    on_respond: EventHandler<InteractionResponse>,
) -> Element {
    let mut value = use_signal(String::new);
    let kind = if request.secret == Some(true) {
        "password"
    } else {
        "text"
    };
    rsx! {
        form {
            class: "interaction-form",
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                on_respond.call(InteractionResponse::Value(value()));
            },
            input {
                r#type: kind,
                "data-testid": "interaction-input",
                "aria-label": "{request.title}",
                placeholder: request.placeholder.clone().unwrap_or_default(),
                disabled,
                value: "{value}",
                onmounted: move |event| {
                    if autofocus {
                        focus(&event);
                    }
                },
                oninput: move |event| value.set(event.value()),
            }
            button { r#type: "submit", "data-testid": "interaction-submit", disabled, "Submit" }
            button {
                r#type: "button",
                class: "secondary",
                "data-testid": "interaction-dismiss",
                disabled,
                onclick: move |_| on_respond.call(InteractionResponse::Cancelled),
                "Dismiss"
            }
        }
    }
}

#[component]
fn EditorBody(
    request: EditorUiRequest,
    disabled: bool,
    autofocus: bool,
    on_respond: EventHandler<InteractionResponse>,
) -> Element {
    let prefill = request.prefill.clone().unwrap_or_default();
    let mut value = use_signal(move || prefill);
    rsx! {
        form {
            class: "interaction-form",
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                on_respond.call(InteractionResponse::Value(value()));
            },
            textarea {
                "data-testid": "interaction-editor",
                "aria-label": "{request.title}",
                rows: "8",
                disabled,
                value: "{value}",
                onmounted: move |event| {
                    if autofocus {
                        focus(&event);
                    }
                },
                oninput: move |event| value.set(event.value()),
            }
            button { r#type: "submit", "data-testid": "interaction-submit", disabled, "Submit" }
            button {
                r#type: "button",
                class: "secondary",
                "data-testid": "interaction-dismiss",
                disabled,
                onclick: move |_| on_respond.call(InteractionResponse::Cancelled),
                "Dismiss"
            }
        }
    }
}

/// Changes the `ask` form `form_key`, starting from an empty one.
fn edit_ask(
    mut ui: Signal<crate::UiState>,
    form_key: &InteractionKey,
    request: &AskUiRequest,
    change: impl FnOnce(&mut AskForm, &AskUiRequest),
) {
    let mut state = ui.write();
    let form = state
        .ask_forms
        .entry(form_key.clone())
        .or_insert_with(|| AskForm::new(request));
    change(form, request);
}

/// The `ask` form: every question takes options and free text; single/multi rules are checked before sending.
#[component]
fn AskBody(
    form_key: InteractionKey,
    request: AskUiRequest,
    disabled: bool,
    autofocus: bool,
    on_respond: EventHandler<InteractionResponse>,
) -> Element {
    let ctx = Ctx::get();
    let ui = ctx.ui;
    let form = ui
        .read()
        .ask_forms
        .get(&form_key)
        .cloned()
        .unwrap_or_else(|| AskForm::new(&request));
    let checked = form.check(&request);
    let error = checked.as_ref().err().cloned();
    let submit_request = request.clone();
    let submit_key = form_key.clone();
    rsx! {
        form {
            class: "interaction-form ask",
            "data-testid": "ask-form",
            onsubmit: move |event: FormEvent| {
                event.prevent_default();
                let form = ui.peek().ask_forms.get(&submit_key).cloned().unwrap_or_else(|| AskForm::new(&submit_request));
                if let Ok(answers) = form.check(&submit_request) {
                    on_respond.call(InteractionResponse::Answers(answers));
                }
            },
            for (index, question) in request.questions.iter().enumerate() {
                AskQuestionView {
                    key: "{question.id}",
                    form_key: form_key.clone(),
                    request: request.clone(),
                    index,
                    draft: form.answers.get(index).cloned().unwrap_or_default(),
                    disabled,
                    autofocus: autofocus && index == 0,
                }
            }
            if let Some(error) = error {
                p { class: "form-error", "data-testid": "ask-error", "{error}" }
            }
            button { r#type: "submit", "data-testid": "interaction-submit", disabled: disabled || checked.is_err(), "Send answers" }
            button {
                r#type: "button",
                class: "secondary",
                "data-testid": "interaction-dismiss",
                disabled,
                onclick: move |_| on_respond.call(InteractionResponse::Cancelled),
                "Dismiss"
            }
        }
    }
}

/// One `ask` question: its options (radio or checkbox by its rule) and free text.
#[component]
fn AskQuestionView(
    form_key: InteractionKey,
    request: AskUiRequest,
    index: usize,
    draft: AnswerDraft,
    disabled: bool,
    autofocus: bool,
) -> Element {
    let ui = Ctx::get().ui;
    let Some(question) = request.questions.get(index).cloned() else {
        return rsx! {};
    };
    let (custom_key, custom_request) = (form_key.clone(), request.clone());
    rsx! {
        fieldset { class: "ask-question", "data-testid": "ask-question", "data-multi": question.multi,
            legend {
                if let Some(header) = question.header.as_ref() {
                    span { class: "ask-header", "{header}: " }
                }
                "{question.question}"
                span { class: "ask-rule", if question.multi { " (any number)" } else { " (one)" } }
            }
            for (position, option) in question.options.iter().cloned().enumerate() {
                label { key: "{position}", class: "ask-option",
                    input {
                        r#type: if question.multi { "checkbox" } else { "radio" },
                        name: "{form_key.1}-{index}",
                        "data-testid": "ask-option",
                        disabled,
                        checked: draft.selected.contains(&option.label),
                        onmounted: move |event| {
                            if autofocus && position == 0 {
                                focus(&event);
                            }
                        },
                        onchange: {
                            let (form_key, request, label) = (form_key.clone(), request.clone(), option.label.clone());
                            move |_| edit_ask(ui, &form_key, &request, |form, request| form.toggle(request, index, &label))
                        },
                    }
                    span { "{option.label}" }
                    if question.recommended.and_then(|recommended| usize::try_from(recommended).ok()) == Some(position) {
                        span { class: "tag", "recommended" }
                    }
                    if let Some(description) = option.description.as_ref() {
                        span { class: "option-description", "{description}" }
                    }
                }
            }
            input {
                r#type: "text",
                class: "ask-custom",
                "data-testid": "ask-custom",
                "aria-label": "Other answer to {question.question}",
                placeholder: "Other answer",
                disabled,
                value: "{draft.custom}",
                oninput: move |event: FormEvent| {
                    let text = event.value();
                    edit_ask(ui, &custom_key, &custom_request, |form, request| form.set_custom(request, index, &text));
                },
            }
        }
    }
}
