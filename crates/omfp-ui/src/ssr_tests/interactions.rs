//! Pending interactions, `ask` forms, requests closed elsewhere, countdowns.

use omfp_core::wire::ExtensionUiRequest;
use omfp_core::{Resolution, ThreadState};
use serde_json::json;

use super::{Page, attr, index, normalize, render, selecting, text, with_open};
use crate::test_support::fixture_until;
use crate::vm::interaction::AskForm;
use crate::{ClosedInteraction, ViewInput};

fn pending_approval() -> ThreadState {
    fixture_until("approval_granted", |event| {
        event.get("kind") == Some(&json!("interaction_resolved"))
    })
}

fn ask_request() -> ExtensionUiRequest {
    serde_json::from_value(json!({
        "method": "ask",
        "id": "ask-1",
        "timeout": 30000,
        "questions": [
            {"id": "color", "question": "Which color?", "options": [{"label": "red"}, {"label": "blue", "description": "calm"}],
             "recommended": 1},
            {"id": "tools", "question": "Which tools?", "multi": true, "options": [{"label": "bash"}, {"label": "read"}]}
        ]
    }))
    .expect("ask request")
}

fn with_ask() -> ViewInput {
    let state = ThreadState {
        interactions: vec![ask_request()],
        ..ThreadState::default()
    };
    with_open(index(), "t-checkout", state)
}

// spec: UI-070
#[test]
fn a_pending_approval_renders_inline_at_the_bottom_of_the_transcript() {
    let page = Page::new(
        with_open(index(), "t-checkout", pending_approval()),
        selecting("t-checkout"),
    );
    let inside = page.all("[data-testid=\"transcript\"] [data-testid=\"interaction\"]");
    assert_eq!(inside.len(), 1);
    assert_eq!(attr(inside[0], "data-kind"), Some("select"));
    assert!(
        text(inside[0]).starts_with("Allow tool: bash Command: echo approved-run"),
        "{}",
        text(inside[0])
    );
    let options: Vec<String> = page
        .testid("interaction-option")
        .into_iter()
        .map(text)
        .collect();
    assert_eq!(
        options,
        vec!["Approve", "Approve bash for this session", "Deny"]
    );
    page.one("interaction-dismiss");
    // The interactions come after every transcript item.
    let html = render(
        with_open(index(), "t-checkout", pending_approval()),
        selecting("t-checkout"),
    );
    let last_item = html
        .rfind("data-testid=\"transcript-item\"")
        .expect("an item");
    assert!(
        html.find("data-testid=\"interactions\"")
            .is_some_and(|position| position > last_item)
    );
}

// spec: UI-070
#[test]
fn an_answered_interaction_waits_for_the_daemon_with_its_controls_disabled() {
    let mut ui = selecting("t-checkout");
    ui.answered_here
        .insert(("t-checkout".to_owned(), "<id>".to_owned()));
    let page = Page::new(with_open(index(), "t-checkout", pending_approval()), ui);
    assert_eq!(attr(page.one("interaction"), "aria-busy"), Some("true"));
    for option in page.testid("interaction-option") {
        assert_eq!(attr(option, "disabled"), Some("true"));
    }
}

// spec: UI-071
#[test]
fn ask_offers_free_text_for_every_question_and_refuses_incomplete_answers() {
    let page = Page::new(with_ask(), selecting("t-checkout"));
    let questions = page.testid("ask-question");
    assert_eq!(
        questions
            .iter()
            .map(|question| attr(*question, "data-multi"))
            .collect::<Vec<_>>(),
        vec![Some("false"), Some("true")]
    );
    let kinds: Vec<Option<&str>> = page
        .testid("ask-option")
        .into_iter()
        .map(|option| attr(option, "type"))
        .collect();
    assert_eq!(
        kinds,
        vec![
            Some("radio"),
            Some("radio"),
            Some("checkbox"),
            Some("checkbox")
        ]
    );
    assert_eq!(page.testid("ask-custom").len(), 2);
    assert!(text(questions[0]).contains("recommended"));
    assert_eq!(text(page.one("ask-error")), "Answer “Which color?”");
    assert_eq!(
        attr(page.one("interaction-submit"), "disabled"),
        Some("true")
    );
}

// spec: UI-071
#[test]
fn a_complete_ask_form_can_be_sent() {
    let ExtensionUiRequest::Ask(request) = ask_request() else {
        panic!("ask")
    };
    let mut form = AskForm::new(&request);
    form.toggle(&request, 0, "blue");
    form.set_custom(&request, 1, "grep");
    let mut ui = selecting("t-checkout");
    ui.ask_forms
        .insert(("t-checkout".to_owned(), "ask-1".to_owned()), form);
    let page = Page::new(with_ask(), ui);
    assert_eq!(page.testid("ask-error").len(), 0);
    assert_ne!(
        attr(page.one("interaction-submit"), "disabled"),
        Some("true")
    );
    let checked: Vec<Option<&str>> = page
        .testid("ask-option")
        .into_iter()
        .map(|option| attr(option, "checked"))
        .collect();
    // Server-side rendering writes `checked=true/` for a checked self-closing input.
    assert!(
        checked[1].is_some_and(|checked| checked.starts_with("true")),
        "{checked:?}"
    );
    assert_eq!(checked[0], None);
    assert_eq!(attr(page.testid("ask-custom")[1], "value"), Some("grep"));
}

// spec: UI-072
#[test]
fn a_request_answered_by_another_client_closes_and_says_who_answered() {
    let request = match pending_approval().interactions.first() {
        Some(request) => request.clone(),
        None => panic!("pending approval"),
    };
    let mut input = with_open(
        index(),
        "t-checkout",
        fixture_until("approval_granted", |_| false),
    );
    if let Some(open) = input.open.get_mut("t-checkout") {
        open.closed.push(ClosedInteraction {
            request,
            resolution: Some(Resolution::Client {
                client: "contract-test".to_owned(),
            }),
        });
    }
    let page = Page::new(input.clone(), selecting("t-checkout"));
    assert_eq!(page.testid("interaction").len(), 0);
    let closed = page.one("interaction-closed");
    assert!(
        text(closed).ends_with("— Answered elsewhere by contract-test×"),
        "{}",
        text(closed)
    );
    let mut ui = selecting("t-checkout");
    ui.answered_here
        .insert(("t-checkout".to_owned(), "<id>".to_owned()));
    assert_eq!(
        Page::new(input, ui).testid("interaction-closed").len(),
        0,
        "answered from this window"
    );
}

// spec: UI-073
#[test]
fn a_request_with_a_timeout_shows_a_countdown() {
    let page = Page::new(with_ask(), selecting("t-checkout"));
    let countdown = page.one("countdown");
    assert_eq!(
        (attr(countdown, "role"), text(countdown)),
        (Some("timer"), "0:30".to_owned())
    );
    let approval = Page::new(
        with_open(index(), "t-checkout", pending_approval()),
        selecting("t-checkout"),
    );
    assert_eq!(
        approval.testid("countdown").len(),
        0,
        "the approval has no timeout"
    );
}

// spec: TST-031, TST-032
#[test]
fn ask_snapshot() {
    insta::assert_snapshot!(normalize(&render(with_ask(), selecting("t-checkout"))));
}
