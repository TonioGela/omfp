//! The access-token prompt (UI-002): shown on the web when no token is known or the daemon rejected it. Like every
//! dialog it traps focus (UI-092); the host makes the page behind it inert.

use std::rc::Rc;

use dioxus::prelude::*;

fn focus(target: Signal<Option<Rc<MountedData>>>) {
    if let Some(target) = target.peek().clone() {
        spawn(async move {
            // Focus is best effort: the element may be gone.
            let _ = target.set_focus(true).await;
        });
    }
}

/// Asks for the token printed by `omfp daemon`; `rejected` says the previous one was refused.
#[component]
pub fn TokenPrompt(rejected: bool, on_submit: EventHandler<String>) -> Element {
    let mut token = use_signal(String::new);
    let first: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    let last: Signal<Option<Rc<MountedData>>> = use_signal(|| None);
    rsx! {
        div { class: "dialog-backdrop",
            div {
                class: "dialog",
                role: "dialog",
                "aria-modal": "true",
                "aria-labelledby": "token-heading",
                "data-testid": "token-dialog",
                div { tabindex: "0", "data-testid": "focus-start", onfocus: move |_| focus(last) }
                h2 { id: "token-heading", "Connect to the daemon" }
                p {
                    "Open the link printed by "
                    code { "omfp daemon" }
                    ", or paste the access token from its data directory."
                }
                form {
                    onsubmit: move |event: FormEvent| {
                        event.prevent_default();
                        let value = token.peek().trim().to_owned();
                        if !value.is_empty() {
                            on_submit.call(value);
                        }
                    },
                    label {
                        "Access token"
                        input {
                            r#type: "password",
                            autocomplete: "off",
                            "data-testid": "token-input",
                            value: "{token}",
                            onmounted: move |event| {
                                let mut first = first;
                                first.set(Some(event.data()));
                                focus(first);
                            },
                            oninput: move |event| token.set(event.value()),
                        }
                    }
                    if rejected {
                        p { class: "form-error", role: "alert", "data-testid": "token-rejected",
                            "The daemon rejected this token."
                        }
                    }
                    div { class: "dialog-actions",
                        button {
                            r#type: "submit",
                            class: "primary",
                            "data-testid": "token-submit",
                            onmounted: move |event| {
                                let mut last = last;
                                last.set(Some(event.data()));
                            },
                            "Connect"
                        }
                    }
                }
                div { tabindex: "0", "data-testid": "focus-end", onfocus: move |_| focus(first) }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use scraper::{Html, Selector};

    use super::*;

    fn render(rejected: bool) -> Html {
        #[derive(Props, Clone, PartialEq)]
        struct HarnessProps {
            rejected: bool,
        }
        #[allow(non_snake_case)]
        fn Harness(HarnessProps { rejected }: HarnessProps) -> Element {
            rsx! {
                TokenPrompt { rejected, on_submit: move |_| {} }
            }
        }
        let mut dom = VirtualDom::new_with_props(Harness, HarnessProps { rejected });
        dom.rebuild_in_place();
        Html::parse_fragment(&dioxus_ssr::render(&dom))
    }

    fn count(html: &Html, testid: &str) -> usize {
        let selector = Selector::parse(&format!("[data-testid=\"{testid}\"]")).unwrap();
        html.select(&selector).count()
    }

    // spec: UI-002, UI-092
    #[test]
    fn the_token_prompt_is_a_modal_dialog_that_says_when_the_token_was_rejected() {
        let first = render(false);
        let dialog = Selector::parse("[role=dialog][aria-modal=true]").unwrap();
        assert_eq!(first.select(&dialog).count(), 1);
        let password = Selector::parse("input[type=password][data-testid=token-input]").unwrap();
        assert_eq!(first.select(&password).count(), 1);
        assert_eq!(
            (
                count(&first, "focus-start"),
                count(&first, "focus-end"),
                count(&first, "token-rejected")
            ),
            (1, 1, 0)
        );
        assert_eq!(count(&render(true), "token-rejected"), 1);
    }
}
