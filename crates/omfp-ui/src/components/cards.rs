//! Message blocks, tool cards, notices, and the collapsible frame they share.

use dioxus::prelude::*;

use crate::render::markdown::to_safe_html;
use crate::views::Ctx;
use crate::vm::notice::NoticeRow;
use crate::vm::tool::{ToolCard, format_duration};
use crate::vm::transcript::{Block, MessageView, Role};

/// A disclosure whose state lives in the window state under `card_key`, so it survives virtualization and thread
/// switches (UI-010).
#[component]
pub(crate) fn Collapsible(
    card_key: String,
    expanded_by_default: bool,
    label: Element,
    children: Element,
) -> Element {
    let ctx = Ctx::get();
    let mut ui = ctx.ui;
    let expanded = ui.read().is_expanded(&card_key, expanded_by_default);
    let body_id = format!("body-{card_key}");
    let toggle_key = card_key.clone();
    rsx! {
        div { class: "collapsible", "data-expanded": expanded,
            button {
                r#type: "button",
                class: "disclosure",
                "data-testid": "toggle",
                "aria-expanded": expanded,
                "aria-controls": "{body_id}",
                onclick: move |_| ui.write().toggle_expanded(&toggle_key, expanded_by_default),
                span { class: "disclosure-mark", "aria-hidden": "true", if expanded { "▾" } else { "▸" } }
                {label}
            }
            if expanded {
                div { id: "{body_id}", class: "disclosure-body", {children} }
            }
        }
    }
}

/// Assistant Markdown, sanitized before insertion (UI-040). Re-rendered only when the source changes.
#[component]
pub(crate) fn Markdown(source: String) -> Element {
    let html = to_safe_html(&source);
    rsx! {
        div { class: "markdown", "data-testid": "markdown", dangerous_inner_html: "{html}" }
    }
}

/// One message: label, blocks, error.
#[component]
pub(crate) fn MessageBlocks(card_key: String, view: MessageView, streaming: bool) -> Element {
    let role = view.role;
    rsx! {
        article {
            class: "message role-{role.token()}",
            "data-testid": "message",
            "data-role": role.token(),
            "data-streaming": streaming,
            "aria-busy": streaming,
            div { class: "message-label",
                "{view.label}"
                if view.steering {
                    span { class: "tag", "data-testid": "steering-mark", "steering" }
                }
            }
            for (index, block) in view.blocks.into_iter().enumerate() {
                BlockView { key: "{index}", card_key: format!("{card_key}/{index}"), role, block }
            }
            if let Some(error) = view.error {
                p { class: "message-error", "data-testid": "message-error", "{error}" }
            }
        }
    }
}

#[component]
fn BlockView(card_key: String, role: Role, block: Block) -> Element {
    match block {
        Block::Text(text) if role == Role::Assistant => rsx! {
            Markdown { source: text }
        },
        Block::Text(text) => rsx! {
            div { class: "plain", "data-testid": "plain-text", "{text}" }
        },
        Block::Thinking(text) => rsx! {
            div { class: "thinking", "data-testid": "thinking",
                Collapsible { card_key, expanded_by_default: false, label: rsx! { "Thinking" },
                    div { class: "thinking-text", "{text}" }
                }
            }
        },
        Block::Image { mime, data } => rsx! {
            img { class: "message-image", "data-testid": "message-image", src: "data:{mime};base64,{data}", alt: "Attached image" }
        },
        Block::Tool(card) => rsx! {
            ToolCardView { card_key, card }
        },
        Block::Record { summary, json } => rsx! {
            div { class: "record", "data-testid": "record",
                Collapsible { card_key, expanded_by_default: false, label: rsx! { span { class: "record-summary", "{summary}" } },
                    pre { class: "json", "{json}" }
                }
            }
        },
    }
}

/// The fallback tool card (UI-050): collapsible, failed cards distinct and expanded by default (UI-052).
#[component]
pub(crate) fn ToolCardView(card_key: String, card: ToolCard) -> Element {
    let status = card.status;
    let expanded_by_default = card.expanded_by_default();
    rsx! {
        section {
            class: "tool-card status-{status.token()}",
            "data-testid": "tool-card",
            "data-tool": "{card.name}",
            "data-status": status.token(),
            "aria-label": "Tool {card.name}",
            Collapsible { card_key, expanded_by_default,
                label: rsx! {
                    span { class: "tool-name", "{card.name}" }
                    span { class: "tool-summary", "data-testid": "tool-summary", "{card.summary}" }
                    span { class: "tool-status", "data-testid": "tool-status", "{status.label()}" }
                    if let Some(duration) = card.duration_ms {
                        span { class: "tool-duration", "{format_duration(duration)}" }
                    }
                },
                div { class: "tool-section",
                    h4 { "Arguments" }
                    pre { class: "json", "data-testid": "tool-args", "{card.args}" }
                }
                if let Some(result) = card.result {
                    div { class: "tool-section",
                        h4 { "Result" }
                        pre { class: "tool-result", "data-testid": "tool-result", "{result}" }
                    }
                }
            }
        }
    }
}

/// An inline notice row (UI-043); unknown frames expand to their JSON.
#[component]
pub(crate) fn NoticeRowView(card_key: String, row: NoticeRow) -> Element {
    rsx! {
        div {
            class: "notice level-{row.level.token()}",
            "data-testid": "notice",
            "data-kind": row.kind,
            "data-level": row.level.token(),
            span { class: "notice-text", "{row.text}" }
            if let Some(json) = row.json {
                Collapsible { card_key, expanded_by_default: false, label: rsx! { "Details" },
                    pre { class: "json", "data-testid": "notice-json", "{json}" }
                }
            }
        }
    }
}
