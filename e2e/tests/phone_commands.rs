//! Commands on a phone (E2E-39, UI-460..UI-465) in headless Chromium emulating a 412 × 915 touch phone through
//! Chrome's debugging protocol (`Emulation.*`: a mobile viewport, touch, so `(pointer: coarse)` matches): the area
//! bar's Commands button opens the full-screen sheet, whose rows run commands or open the `:` line on their name.
//! Screenshots land in `/tmp` as `omfp-phone-commands-<state>.png` (or `OMFP_SCREENSHOT_DIR`); nothing compares pixels.

use e2e::Journey;
use serde_json::{Value, json};
use thirtyfour::prelude::Key;

const WIDTH: u32 = 412;
const HEIGHT: u32 = 915;
const SHEET: &str = "[data-testid=commands-sheet]";
const COMMANDS: &str = "[data-testid=area-action][data-action=commands]";

/// Emulates a touch phone and waits for the phone layout.
async fn phone(journey: &Journey) -> anyhow::Result<()> {
    let cdp = journey.driver.cdp();
    cdp.send_raw(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": WIDTH, "height": HEIGHT, "deviceScaleFactor": 2, "mobile": true }),
    )
    .await?;
    cdp.send_raw(
        "Emulation.setTouchEmulationEnabled",
        json!({ "enabled": true, "maxTouchPoints": 5 }),
    )
    .await?;
    journey
        .until("the phone layout", async |page| {
            let value = page.attr("[data-testid=app]", "data-layout").await?;
            Ok((value.as_deref() == Some("mobile")).then_some(()))
        })
        .await
}

/// Once its entrance has played: the sheet's box and the window's size, then the lowest row and the close button's
/// size, in px.
async fn measure(journey: &Journey) -> anyhow::Result<(Vec<f64>, f64, Vec<f64>)> {
    let value: Value = journey
        .eval(
            "const sheet = document.querySelector('.omfp-sheet-overlay');
             return Promise.all(sheet.getAnimations({ subtree: true }).map((animation) => animation.finished)).then(() => {
               const box = sheet.getBoundingClientRect();
               const rows = [...document.querySelectorAll('[data-testid=commands-row]')]
                 .map((row) => row.getBoundingClientRect().height);
               const close = document.querySelector('[data-testid=sheet-close]').getBoundingClientRect();
               return [[box.left, box.top, box.width, box.height, innerWidth, innerHeight], Math.min(...rows),
                 [close.width, close.height]];
             });",
            Vec::new(),
        )
        .await?;
    Ok(serde_json::from_value(value)?)
}

/// The sheet covers the window and its targets are touch-sized; the search narrows it, and a command without
/// arguments (`:help`) runs as it closes.
async fn run_from_the_sheet(journey: &Journey) -> anyhow::Result<()> {
    journey.click(COMMANDS).await?;
    journey.element(SHEET).await?;
    let (window, lowest, close) = measure(journey).await?;
    let [left, top, width, height, inner_width, inner_height] = window[..] else {
        anyhow::bail!("measured {window:?}");
    };
    anyhow::ensure!(
        [left, top, width - inner_width, height - inner_height]
            .iter()
            .all(|gap| gap.abs() < 0.5),
        "the sheet is not the whole window: {window:?}"
    );
    anyhow::ensure!(lowest >= 44.0, "a row is {lowest} px tall");
    anyhow::ensure!(
        close.iter().all(|side| *side >= 44.0),
        "the close button is {close:?}"
    );
    let all = journey.count("[data-testid=commands-row]").await?;
    journey.screenshot("phone-commands-sheet").await?;
    journey
        .type_into("[data-testid=commands-search]", "help")
        .await?;
    journey
        .until("the search to narrow the rows", async |page| {
            let count = page.count("[data-testid=commands-row]").await?;
            Ok((count > 0 && count < all).then_some(()))
        })
        .await?;
    journey.screenshot("phone-commands-search").await?;
    journey
        .click("[data-testid=commands-row][data-command='help.open']")
        .await?;
    journey.gone(SHEET).await?;
    journey.element("[data-testid=help]").await?;
    journey.screenshot("phone-commands-help").await?;
    journey.keys(Key::Escape).await?;
    journey.gone("[data-testid=help]").await
}

// spec: E2E-39, UI-460, UI-461, UI-462, UI-463, UI-464, UI-465
#[tokio::test(flavor = "multi_thread")]
async fn a_phone_runs_commands_from_the_commands_sheet() -> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    journey.create_thread("phone commands journey").await?;
    phone(&journey).await?;
    let pill = journey
        .eval(
            "return getComputedStyle(document.querySelector('[data-testid=mode-line]')).display;",
            Vec::new(),
        )
        .await?;
    assert_eq!(
        pill.as_str(),
        Some("none"),
        "the mode pill shows on a phone"
    );
    journey.screenshot("phone-commands-bar").await?;

    run_from_the_sheet(&journey).await?;

    // A command taking arguments opens the `:` line on its name; its completion runs it.
    journey.click(COMMANDS).await?;
    journey
        .click("[data-testid=commands-row][data-command='theme.set']")
        .await?;
    journey.gone(SHEET).await?;
    journey
        .element("[data-testid=command-option][data-value='theme dark']")
        .await?;
    let line = journey
        .eval(
            "return document.querySelector('[data-testid=command-line-input]').value;",
            Vec::new(),
        )
        .await?;
    assert_eq!(line.as_str(), Some("theme "));
    journey.screenshot("phone-commands-cmdline").await?;
    journey
        .click("[data-testid=command-option][data-value='theme dark']")
        .await?;
    journey.until_scheme("dark").await?;

    // The browser's back (a phone's back gesture) closes the sheet, not the app.
    journey.click(COMMANDS).await?;
    journey.element(SHEET).await?;
    journey.driver.back().await?;
    journey.gone(SHEET).await?;
    journey.element("[data-testid=area-bar]").await?;
    journey.finish("E2E-39").await
}
