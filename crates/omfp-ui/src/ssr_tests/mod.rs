//! L4 component tests: the app rendered with `dioxus-ssr` from thread states replayed from the recorded fixtures and
//! hand-built DTOs, asserted with `data-testid` selectors (TST-031) and normalized HTML snapshots (TST-032).

use std::sync::LazyLock;

use dioxus::prelude::*;
use omfp_core::{ProcessState, ThreadState};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};

use crate::test_support::{repository, thread, workspace};
use crate::{App, ConnectionView, ThreadView, UiState, ViewInput};

mod composer;
mod interactions;
mod shell;
mod transcript;

#[derive(Props, Clone, PartialEq)]
struct HarnessProps {
    input: ViewInput,
    initial: UiState,
}

#[allow(non_snake_case)]
fn Harness(HarnessProps { input, initial }: HarnessProps) -> Element {
    let input = use_signal(move || input);
    rsx! {
        App { input, on_intent: move |_| {}, initial }
    }
}

/// The app's HTML for `input` and window state `initial`.
pub fn render(input: ViewInput, initial: UiState) -> String {
    let mut dom = VirtualDom::new_with_props(Harness, HarnessProps { input, initial });
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

/// Parsed HTML with `data-testid` helpers.
pub struct Page {
    html: Html,
}

impl Page {
    pub fn new(input: ViewInput, initial: UiState) -> Self {
        Self {
            html: Html::parse_fragment(&render(input, initial)),
        }
    }

    /// Elements matching a CSS selector.
    pub fn all(&self, selector: &str) -> Vec<ElementRef<'_>> {
        let selector =
            Selector::parse(selector).unwrap_or_else(|error| panic!("{selector}: {error}"));
        self.html.select(&selector).collect()
    }

    /// Elements with `data-testid`.
    pub fn testid(&self, testid: &str) -> Vec<ElementRef<'_>> {
        self.all(&format!("[data-testid=\"{testid}\"]"))
    }

    /// The single element with `data-testid`.
    pub fn one(&self, testid: &str) -> ElementRef<'_> {
        let found = self.testid(testid);
        assert_eq!(
            found.len(),
            1,
            "expected one [data-testid={testid}], found {}",
            found.len()
        );
        found[0]
    }
}

/// An element's text, whitespace-collapsed.
pub fn text(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join("")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// An attribute's value.
pub fn attr<'a>(element: ElementRef<'a>, name: &str) -> Option<&'a str> {
    element.value().attr(name)
}

/// An index of two repositories, four workspaces and four threads: `t-checkout` awaits input (fe#412 + be:main,
/// tagged `release`), `t-flaky` runs (be:main + be:flaky), `t-scratch` failed (no workspace), `t-idle` is idle on an
/// adopted directory.
pub fn index() -> ViewInput {
    let mut pr = workspace("w-fe", Some("r-fe"), Some("feat/checkout"));
    pr.pr_number = Some(412);
    pr.pr_url = Some("https://example.test/fe/pull/412".to_owned());
    let mut checkout = thread("t-checkout", "checkout-v2", &["w-fe", "w-be"]);
    checkout.needs_attention = true;
    checkout.tags = vec!["release".to_owned()];
    let mut flaky = thread("t-flaky", "flaky-tests", &["w-be", "w-be2"]);
    flaky.settled = false;
    let mut scratch = thread("t-scratch", "scratch", &[]);
    scratch.process = ProcessState::Failed {
        message: "omp exited".to_owned(),
        exit_code: Some(1),
        stderr_tail: String::new(),
    };
    ViewInput {
        connection: ConnectionView::Connected,
        repositories: vec![repository("r-fe", "fe"), repository("r-be", "be")],
        workspaces: vec![
            pr,
            workspace("w-be", Some("r-be"), Some("main")),
            workspace("w-be2", Some("r-be"), Some("flaky")),
            workspace("w-adopted", None, None),
        ],
        threads: vec![
            checkout,
            flaky,
            scratch,
            thread("t-idle", "idle-notes", &["w-adopted"]),
        ],
        ..ViewInput::default()
    }
}

/// `input` with `thread_id` open on `state`.
pub fn with_open(mut input: ViewInput, thread_id: &str, state: ThreadState) -> ViewInput {
    input.open.insert(
        thread_id.to_owned(),
        ThreadView {
            state,
            ..ThreadView::default()
        },
    );
    input
}

/// Window state with `thread_id` selected.
pub fn selecting(thread_id: &str) -> UiState {
    UiState {
        selected: Some(thread_id.to_owned()),
        ..UiState::default()
    }
}

static ASSET_HASH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/assets/([\w-]+?)-[0-9a-zA-Z]{8,}\.(\w+)").expect("regex"));
static DATA_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"data:([\w/+.-]+);base64,[A-Za-z0-9+/=]+").expect("regex"));
static COUNTDOWN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(data-testid="countdown"[^>]*>)\d+:\d\d"#).expect("regex"));

/// HTML for snapshots: asset hashes, inline image data and countdown values replaced, one tag per line (TST-032).
pub fn normalize(html: &str) -> String {
    let html = ASSET_HASH.replace_all(html, "/assets/$1.$2");
    let html = DATA_URL.replace_all(&html, "data:$1;base64,[data]");
    let html = COUNTDOWN.replace_all(&html, "${1}[m:ss]");
    html.replace("><", ">\n<")
}

// spec: TST-032
#[test]
fn normalization_strips_volatile_values_and_keeps_the_markup() {
    let html = concat!(
        r#"<link href="/assets/omfp-dxh3f9a8c71e0b2.css"><img src="data:image/png;base64,iVBORw0KGgo="/>"#,
        r#"<span class="countdown" data-testid="countdown" data-expired=false>0:27</span>"#,
    );
    assert_eq!(
        normalize(html),
        concat!(
            "<link href=\"/assets/omfp.css\">\n<img src=\"data:image/png;base64,[data]\"/>\n",
            "<span class=\"countdown\" data-testid=\"countdown\" data-expired=false>[m:ss]</span>",
        )
    );
}
