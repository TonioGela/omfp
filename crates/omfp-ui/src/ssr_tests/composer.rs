//! Composer, queue chips and drafts.

use omfp_core::ThreadState;

use super::{Page, attr, index, selecting, text, with_open};
use crate::intent::QueueOp;
use crate::test_support::{fixture, fixture_prefix};
use crate::vm::composer::{Draft, image_attachment};
use crate::vm::queue::PendingQueueOp;
use crate::{UiState, ViewInput};

/// Events of `follow_up_then_abort` through its first `queue_update`: busy, one follow-up queued.
const QUEUED: usize = 12;

fn draft(text: &str) -> Draft {
    Draft {
        text: text.to_owned(),
        images: Vec::new(),
    }
}

fn with_draft(thread_id: &str, draft: Draft) -> UiState {
    let mut ui = selecting(thread_id);
    ui.drafts.insert(thread_id.to_owned(), draft);
    ui
}

fn busy() -> ViewInput {
    with_open(
        index(),
        "t-checkout",
        fixture_prefix("follow_up_then_abort", QUEUED),
    )
}

// spec: UI-060
#[test]
fn the_composer_is_multi_line_and_completes_slash_commands_from_the_thread() {
    let state = fixture("text_reply");
    let first = state.commands[0].name.clone();
    let input = with_open(index(), "t-checkout", state);
    let page = Page::new(
        input.clone(),
        with_draft("t-checkout", draft(&format!("/{first}"))),
    );
    let textarea = page.one("composer-input");
    assert_eq!(textarea.value().name(), "textarea");
    assert_eq!(attr(textarea, "rows"), Some("3"));
    let suggestions: Vec<String> = page
        .testid("slash-suggestion")
        .into_iter()
        .map(text)
        .collect();
    assert!(
        suggestions
            .first()
            .is_some_and(|suggestion| suggestion.starts_with(&format!("/{first}"))),
        "{suggestions:?}"
    );
    let typing = Page::new(
        input,
        with_draft("t-checkout", draft(&format!("/{first} now"))),
    );
    assert_eq!(typing.testid("slash-suggestions").len(), 0);
}

// spec: UI-060
#[test]
fn attached_images_show_in_the_composer() {
    let image = image_attachment("image/png", &[1, 2, 3]).expect("image");
    let ui = with_draft(
        "t-checkout",
        Draft {
            text: String::new(),
            images: vec![image],
        },
    );
    let page = Page::new(with_open(index(), "t-checkout", fixture("text_reply")), ui);
    let attachment = page.one("attachment");
    let img = attachment
        .select(&scraper::Selector::parse("img").expect("selector"))
        .next()
        .expect("img");
    assert_eq!(attr(img, "src"), Some("data:image/png;base64,AQID"));
}

// spec: UI-061
#[test]
fn the_send_button_shows_whether_a_busy_thread_queues_or_steers() {
    let idle = Page::new(
        with_open(index(), "t-checkout", fixture("text_reply")),
        selecting("t-checkout"),
    );
    assert_eq!(
        (text(idle.one("send")), attr(idle.one("send"), "data-mode")),
        ("Send".to_owned(), Some("prompt"))
    );
    assert_eq!(idle.testid("send-alternative").len(), 0);
    let queued = Page::new(busy(), selecting("t-checkout"));
    assert_eq!(
        (
            text(queued.one("send")),
            attr(queued.one("send"), "data-mode")
        ),
        ("Queue follow-up".to_owned(), Some("follow-up"))
    );
    assert_eq!(text(queued.one("send-alternative")), "Steer now");
    let mut ui = selecting("t-checkout");
    ui.steer_modifier = true;
    let steering = Page::new(busy(), ui);
    assert_eq!(
        (
            text(steering.one("send")),
            attr(steering.one("send"), "data-mode")
        ),
        ("Steer now".to_owned(), Some("steer"))
    );
    assert_eq!(text(steering.one("send-alternative")), "Queue follow-up");
}

// spec: UI-062
#[test]
fn queue_chips_come_from_the_last_queue_update_and_wait_for_confirmation() {
    let page = Page::new(busy(), selecting("t-checkout"));
    let chip = page.one("queue-chip");
    assert_eq!(attr(chip, "data-queue"), Some("follow-up"));
    assert!(text(chip).contains("later please"));
    assert_eq!(attr(chip, "data-pending"), None);
    page.one("queue-promote");
    page.one("queue-remove");
    let mut ui = selecting("t-checkout");
    ui.pending_queue.push(PendingQueueOp {
        thread_id: "t-checkout".to_owned(),
        op: QueueOp::Remove,
        message: "later please".to_owned(),
    });
    let pending = Page::new(busy(), ui.clone());
    let chip = pending.one("queue-chip");
    assert_eq!(
        (attr(chip, "data-pending"), attr(chip, "aria-busy")),
        (Some("remove"), Some("true"))
    );
    assert_eq!(attr(pending.one("queue-remove"), "disabled"), Some("true"));
    // The daemon's next `queue_update` (empty) removes the chip.
    let drained = Page::new(
        with_open(index(), "t-checkout", fixture("follow_up_then_abort")),
        ui,
    );
    assert_eq!(drained.testid("queue-chip").len(), 0);
}

// spec: UI-063
#[test]
fn a_busy_thread_offers_abort() {
    let page = Page::new(busy(), selecting("t-checkout"));
    assert_eq!(text(page.one("abort")), "Abort (Esc)");
    assert_eq!(
        attr(page.one("composer-input"), "aria-keyshortcuts"),
        Some("Enter Shift+Enter Alt+Enter Escape Tab")
    );
    let idle = Page::new(
        with_open(index(), "t-checkout", fixture("text_reply")),
        selecting("t-checkout"),
    );
    assert_eq!(idle.testid("abort").len(), 0);
}

// spec: UI-064, UI-010
#[test]
fn each_thread_keeps_its_own_draft() {
    let input = with_open(
        with_open(index(), "t-checkout", fixture("text_reply")),
        "t-idle",
        ThreadState::default(),
    );
    let mut ui = selecting("t-checkout");
    ui.drafts
        .insert("t-checkout".to_owned(), draft("checkout draft"));
    ui.drafts.insert("t-idle".to_owned(), draft("notes draft"));
    let checkout = Page::new(input.clone(), ui.clone());
    assert_eq!(
        attr(checkout.one("composer-input"), "value"),
        Some("checkout draft")
    );
    ui.selected = Some("t-idle".to_owned());
    let notes = Page::new(input, ui);
    assert_eq!(
        attr(notes.one("composer-input"), "value"),
        Some("notes draft")
    );
}
