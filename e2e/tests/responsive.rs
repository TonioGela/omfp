//! The responsive frame (UI-200..UI-209) in headless Chromium at 1400, 900 and 390 px: wide keeps the left column and
//! the splitters; narrow collapses the column to a strip that opens over the content and keeps the chat header on one
//! row; a phone shows one area at a time behind a bottom bar, without splitters. Screenshots land in `/tmp` as
//! `omfp-responsive-<width>-<scheme>.png` (or `OMFP_SCREENSHOT_DIR`); nothing compares pixels.
//!
//! A touch phone (E2E-36) emulated over CDP (touch, `mobile` metrics: `pointer: coarse`, `hover: none`) at 412 × 915
//! and sideways at 915 × 412, shots as `omfp-touch-*.png`.

use e2e::Journey;
use fake_llm::{Reply, Step};
use serde_json::json;
use thirtyfour::prelude::*;

/// The first message, whose placeholder title (DMN-073) is too long for the full header's top row at 1400 px.
const LONG_PROMPT: &str =
    "Summarize every responsive breakpoint thoroughly, including narrow phones";

/// The `data-layout` the frame settles on.
async fn layout(journey: &Journey, expected: &str) -> anyhow::Result<()> {
    journey
        .until(&format!("data-layout {expected}"), async |page| {
            let value = page.attr("[data-testid=app]", "data-layout").await?;
            Ok((value.as_deref() == Some(expected)).then_some(()))
        })
        .await
}

/// The rendered height of the first element matching `css`, in px.
async fn height(journey: &Journey, css: &str) -> anyhow::Result<f64> {
    let value = journey
        .eval(
            "return document.querySelector(arguments[0]).getBoundingClientRect().height;",
            vec![serde_json::json!(css)],
        )
        .await?;
    Ok(value.as_f64().unwrap_or_default())
}

async fn shots(journey: &Journey, width: u32) -> anyhow::Result<()> {
    for scheme in ["light", "dark"] {
        journey.scheme(scheme).await?;
        journey.until_scheme(scheme).await?;
        journey
            .screenshot(&format!("responsive-{width}-{scheme}"))
            .await?;
    }
    journey.scheme("light").await?;
    Ok(())
}

/// Wide: the left column, its splitter and the view's splitters; no strip; the full header on two rows.
async fn wide(journey: &Journey) -> anyhow::Result<()> {
    layout(journey, "wide").await?;
    anyhow::ensure!(journey.count("[data-testid=left-column]").await? == 1);
    anyhow::ensure!(journey.count("[data-testid=left-splitter]").await? == 1);
    anyhow::ensure!(journey.count("[data-testid=left-strip]").await? == 0);
    anyhow::ensure!(journey.count("[data-testid=splitter]").await? > 0);
    // The full header keeps its chips and the session controls on one bar (two rows, not three).
    let bar = height(
        journey,
        "[data-testid=thread-header] .omfp-chat-header__bar",
    )
    .await?;
    anyhow::ensure!(bar < 40.0, "the header's bar wraps: {bar} px tall");
    // Its top row stays one line with a title too long for it: the title gives way, the controls do not wrap.
    let top = height(
        journey,
        "[data-testid=thread-header] .omfp-thread-header__top",
    )
    .await?;
    anyhow::ensure!(top < 40.0, "the header's top row wraps: {top} px tall");
    journey.fits("[data-testid=thread-header]").await?;
    // The composer's text and caret start at its bar's first chip (UI-337).
    anyhow::ensure!(journey.composer_aligned("[data-testid=composer]").await? == 1);
    shots(journey, 1400).await
}

/// Narrow (half a screen): the strip, a one-row header with its ⋯ menu, nothing scrolling sideways, side cells at
/// a readable width; the toggle opens the column over the content and Escape closes it.
async fn narrow(journey: &Journey) -> anyhow::Result<()> {
    layout(journey, "narrow").await?;
    journey.element("[data-testid=left-strip]").await?;
    journey.element("[data-testid=header-more]").await?;
    let header = height(journey, "[data-testid=thread-header]").await?;
    anyhow::ensure!(header < 64.0, "the header wraps: {header} px tall");
    journey.fits("[data-testid=view]").await?;
    journey.fits("[data-testid=thread-header]").await?;
    // The ⋯ is on the header's row, inside it, and nothing covers it.
    let more = journey
        .eval(
            "const more = document.querySelector('[data-testid=header-more]').getBoundingClientRect();
             const header = document.querySelector('[data-testid=thread-header]').getBoundingClientRect();
             const hit = document.elementFromPoint(more.left + more.width / 2, more.top + more.height / 2);
             return more.width > 0 && more.right <= header.right + 0.5
               && hit !== null && hit.closest('[data-testid=header-more]') !== null;",
            Vec::new(),
        )
        .await?;
    anyhow::ensure!(
        more.as_bool() == Some(true),
        "the header's ⋯ is cut or covered"
    );
    // Side by side, no cell is squashed under 16rem (UI-203).
    let narrowest = journey
        .eval(
            "const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
             const cells = [...document.querySelectorAll('.omfp-view__row:not([data-tabbed]) > .omfp-view__cell')];
             return Math.min(...cells.map(c => c.getBoundingClientRect().width / rem));",
            Vec::new(),
        )
        .await?;
    let narrowest = narrowest.as_f64().unwrap_or_default();
    anyhow::ensure!(narrowest >= 15.9, "a cell is {narrowest} rem wide");
    shots(journey, 900).await?;
    journey.click("[data-testid=left-toggle]").await?;
    journey
        .element("[data-testid=left-column][data-open]")
        .await?;
    journey.screenshot("responsive-900-open").await?;
    journey.keys(Key::Escape).await?;
    journey.gone("[data-testid=left-column][data-open]").await
}

/// Phone: the bottom bar, no visible splitters, 44 px touch targets, the threads as a full-screen panel.
async fn phone(journey: &Journey) -> anyhow::Result<()> {
    layout(journey, "mobile").await?;
    journey.element("[data-testid=area-bar]").await?;
    anyhow::ensure!(journey.count("[data-testid=left-strip]").await? == 0);
    let splitters = journey
        .eval(
            "return [...document.querySelectorAll('[data-testid=splitter], [data-testid=left-splitter]')]
               .filter(e => e.getBoundingClientRect().width > 0).length;",
            Vec::new(),
        )
        .await?;
    anyhow::ensure!(
        splitters.as_u64() == Some(0),
        "visible splitters: {splitters}"
    );
    journey.fits("[data-testid=app]").await?;
    // Touch targets of at least 44 px; icon buttons at least 44 × 44 (UI-205).
    let lowest = journey
        .eval(
            "return Math.min(...[...document.querySelectorAll('[data-testid=area-button], [data-testid=send], [data-testid=header-more]')]
               .map(e => e.getBoundingClientRect().height));",
            Vec::new(),
        )
        .await?;
    let lowest = lowest.as_f64().unwrap_or_default();
    anyhow::ensure!(lowest >= 44.0, "a touch target is {lowest} px tall");
    let narrowest = journey
        .eval(
            "return Math.min(...[...document.querySelectorAll('[data-testid=send], [data-testid=header-more]')]
               .map(e => e.getBoundingClientRect().width));",
            Vec::new(),
        )
        .await?;
    let narrowest = narrowest.as_f64().unwrap_or_default();
    anyhow::ensure!(narrowest >= 44.0, "an icon button is {narrowest} px wide");
    shots(journey, 390).await?;
    journey
        .click("[data-testid=area-button][data-area=threads]")
        .await?;
    journey
        .element("[data-testid=left-column][data-open]")
        .await?;
    journey.screenshot("responsive-390-threads").await?;
    journey
        .click("[data-testid=area-button][data-area=chat]")
        .await?;
    journey.gone("[data-testid=left-column][data-open]").await
}

/// Measuring helpers of the touch checks: `shown(e)` (laid out and not visually hidden, as React Aria's dismiss
/// buttons are inside a clipped box); `seen(e)`, its opacity with its ancestors'; `small(css, scope, square)`, the
/// shown targets under 44 px tall (or wide too when `square`) as `name w×h` strings.
const TOUCH_JS: &str = "
const clipped = (e) => { for (let n = e; n; n = n.parentElement) if (getComputedStyle(n).clipPath !== 'none') return true; return false; };
const shown = (e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0 && !clipped(e); };
const seen = (e) => { let o = 1; for (let n = e; n; n = n.parentElement) o *= Number(getComputedStyle(n).opacity); return o; };
const name = (e) => e.dataset.testid ?? e.getAttribute('aria-label') ?? e.textContent.trim().slice(0, 24);
const small = (css, scope, square) => [...scope.querySelectorAll(css)].filter(shown)
  .map((e) => [e, e.getBoundingClientRect()])
  .filter(([, r]) => r.height < 43.5 || (square && r.width < 43.5))
  .map(([e, r]) => `${name(e)} ${Math.round(r.width)}x${Math.round(r.height)}`);
";

/// Runs `body` after [`TOUCH_JS`] and returns the list of offenders it returns, failing with `what` unless empty.
async fn none(journey: &Journey, what: &str, body: &str) -> anyhow::Result<()> {
    let value = journey
        .eval(&format!("{TOUCH_JS}\n{body}"), Vec::new())
        .await?;
    let offenders = value.as_array().cloned().unwrap_or_default();
    anyhow::ensure!(offenders.is_empty(), "{what}: {offenders:?}");
    Ok(())
}

/// Emulates a touch phone of `width` × `height` px: touch events, a coarse primary pointer, no hover.
async fn touch_screen(journey: &Journey, width: u32, height: u32) -> anyhow::Result<()> {
    let cdp = journey.driver.cdp();
    cdp.send_raw(
        "Emulation.setTouchEmulationEnabled",
        json!({ "enabled": true, "maxTouchPoints": 5 }),
    )
    .await?;
    cdp.send_raw(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": width, "height": height, "deviceScaleFactor": 1, "mobile": true }),
    )
    .await?;
    journey
        .until("a touch-only screen", async |page| {
            let value = page
                .eval(
                    "return matchMedia('(hover: none) and (pointer: coarse)').matches;",
                    Vec::new(),
                )
                .await?;
            Ok((value.as_bool() == Some(true)).then_some(()))
        })
        .await
}

/// The chat on a touch phone: the title keeps room in the compact header (UI-419); the header's `⋯` popover, an
/// overlay portalled out of the app, has touch-sized controls (UI-413).
async fn touch_chat(journey: &Journey) -> anyhow::Result<()> {
    let title = journey
        .eval(
            "return document.querySelector('[data-testid=thread-title]').getBoundingClientRect().width;",
            Vec::new(),
        )
        .await?;
    let title = title.as_f64().unwrap_or_default();
    anyhow::ensure!(title >= 120.0, "the chat title is {title} px wide");
    journey.screenshot("touch-412-chat").await?;
    journey.click("[data-testid=header-more]").await?;
    journey.element(".omfp-popdialog").await?;
    none(
        journey,
        "small controls in the header's popover",
        "return small('button, [role=button]', document.querySelector('.omfp-popdialog'), false);",
    )
    .await?;
    none(
        journey,
        "a popover out of the window",
        "const width = visualViewport.width;
         return [...document.querySelectorAll('.omfp-popover')].filter(shown)
           .map((e) => e.getBoundingClientRect()).filter((r) => r.left < 0 || r.right > width + 0.5)
           .map((r) => `${Math.round(r.left)}..${Math.round(r.right)} of ${width}`);",
    )
    .await?;
    journey.screenshot("touch-412-more").await?;
    journey.keys(Key::Escape).await?;
    journey.gone(".omfp-popdialog").await
}

/// The threads on a touch phone: no control a tap can hit is invisible (UI-415), the small icons are touch targets
/// (UI-416), inputs are 16 px or more (UI-417); the workspace's delete dialog has touch-sized buttons (UI-413) and no
/// key chips (UI-418).
async fn touch_threads(journey: &Journey) -> anyhow::Result<()> {
    journey
        .click("[data-testid=area-button][data-area=threads]")
        .await?;
    let column = "document.querySelector('[data-testid=left-column]')";
    journey
        .element("[data-testid=left-column][data-open]")
        .await?;
    none(
        journey,
        "invisible controls",
        &format!(
            "return [...{column}.querySelectorAll('button, [role=button], input')].filter(shown)
               .filter((e) => seen(e) < 0.5).map(name);"
        ),
    )
    .await?;
    none(
        journey,
        "small icon targets",
        "return small(`[data-testid=workspace-menu], [data-testid=delete-workspace], [data-testid=row-delete],
           [data-testid=workspace-collapse]`, document, true);",
    )
    .await?;
    none(
        journey,
        "inputs under 16 px",
        &format!(
            "return [...{column}.querySelectorAll('input')].filter(shown)
               .filter((e) => parseFloat(getComputedStyle(e).fontSize) < 16).map(name);"
        ),
    )
    .await?;
    journey.screenshot("touch-412-threads").await?;
    journey.click("[data-testid=delete-workspace]").await?;
    let dialog = "document.querySelector('[data-testid=delete-workspace-dialog]')";
    journey
        .element("[data-testid=delete-workspace-dialog]")
        .await?;
    none(
        journey,
        "small dialog buttons",
        &format!("return small('button', {dialog}, false);"),
    )
    .await?;
    none(
        journey,
        "key chips on the dialog's buttons",
        &format!("return [...{dialog}.querySelectorAll('.omfp-kbd')].filter(shown).map(name);"),
    )
    .await?;
    journey.screenshot("touch-412-dialog").await?;
    journey
        .click("[data-testid=cancel-delete-workspace]")
        .await?;
    journey.gone("[data-testid=delete-workspace-dialog]").await
}

// spec: E2E-36, UI-413, UI-414, UI-415, UI-416, UI-417, UI-418, UI-419
#[tokio::test(flavor = "multi_thread")]
async fn a_touch_phone_gets_touch_sizes_in_both_orientations() -> anyhow::Result<()> {
    let journey = Journey::start(vec![Step::reply(Reply::text("hello from a phone"))]).await?;
    journey.connected().await?;
    let thread = journey
        .create_thread("a touch journey with a rather long workspace name")
        .await?;
    journey.send(LONG_PROMPT).await?;
    journey.reply("hello from a phone").await?;
    journey.status(&thread, "idle").await?;
    journey.driver.set_window_rect(0, 0, 412, 915).await?;
    touch_screen(&journey, 412, 915).await?;
    layout(&journey, "mobile").await?;
    touch_chat(&journey).await?;
    touch_threads(&journey).await?;
    // Sideways, a phone keeps the phone layout (UI-414).
    journey.driver.set_window_rect(0, 0, 915, 412).await?;
    touch_screen(&journey, 915, 412).await?;
    journey
        .click("[data-testid=area-button][data-area=chat]")
        .await?;
    layout(&journey, "mobile").await?;
    journey.element("[data-testid=area-bar]").await?;
    journey
        .element("[data-testid=thread-header][data-compact]")
        .await?;
    journey.fits("[data-testid=app]").await?;
    journey.screenshot("touch-915-landscape").await?;
    journey.finish("touch").await
}

// spec: UI-200, UI-201, UI-202, UI-203, UI-204, UI-205, UI-206, UI-207, UI-209, UI-337
#[tokio::test(flavor = "multi_thread")]
async fn the_frame_adapts_to_its_width() -> anyhow::Result<()> {
    let journey = Journey::start(vec![Step::reply(Reply::text(
        "hello from the responsive journey",
    ))])
    .await?;
    journey.connected().await?;
    journey.driver.set_window_rect(0, 0, 1400, 1000).await?;
    let thread = journey
        .create_thread("a responsive journey with a rather long workspace name")
        .await?;
    journey.send(LONG_PROMPT).await?;
    journey.reply("hello from the responsive journey").await?;
    journey.status(&thread, "idle").await?;
    wide(&journey).await?;
    journey.driver.set_window_rect(0, 0, 900, 1000).await?;
    narrow(&journey).await?;
    journey.driver.set_window_rect(0, 0, 390, 844).await?;
    phone(&journey).await?;
    journey.finish("responsive").await
}
