//! The `/kit` gallery (UI-173, UI-174) at motion `none`, 17 px, compact, in light and dark for the built-in Synth
//! palette and the matugen fixture theme: every text role, status color and gradient stop the page measures reaches
//! WCAG AA. Full-page shots land in `/tmp/omfp-kit-<theme>-<scheme>.png` (or `OMFP_SCREENSHOT_DIR`) for a look by eye;
//! nothing compares them.

use std::path::PathBuf;

use anyhow::{Context, bail};
use e2e::Journey;
use serde_json::json;

/// The page width of every shot (CSS px, device scale 1).
const WIDTH: u32 = 1400;

const THEMES: [&str; 2] = ["synth", "matugen-forest"];
const SCHEMES: [&str; 2] = ["light", "dark"];

fn shot_path(theme: &str, scheme: &str) -> PathBuf {
    std::env::var_os("OMFP_SCREENSHOT_DIR")
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from)
        .join(format!("omfp-kit-{theme}-{scheme}.png"))
}

/// Opens the gallery with `theme` and `scheme` and waits until it says it is ready (fonts loaded, contrast measured).
async fn open(journey: &Journey, theme: &str, scheme: &str) -> anyhow::Result<()> {
    let base = format!("http://{}", journey.daemon().addr());
    journey
        .driver
        .goto(format!(
            "{base}/kit?scheme={scheme}&theme={theme}&motion=none&density=compact&fs=17"
        ))
        .await?;
    journey
        .until("the gallery to be ready", async |page| {
            Ok((page
                .count("[data-testid=kit-gallery][data-ready=true]")
                .await?
                == 1)
                .then_some(()))
        })
        .await
}

/// A screenshot of the whole page: the viewport is made as tall as the page first.
async fn full_page(journey: &Journey) -> anyhow::Result<Vec<u8>> {
    let cdp = journey.driver.cdp();
    cdp.send_raw(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": WIDTH, "height": 1000, "deviceScaleFactor": 1, "mobile": false }),
    )
    .await?;
    let height = journey
        .eval("return document.documentElement.scrollHeight;", Vec::new())
        .await?
        .as_u64()
        .context("the page height")?;
    cdp.send_raw(
        "Emulation.setDeviceMetricsOverride",
        json!({ "width": WIDTH, "height": height, "deviceScaleFactor": 1, "mobile": false }),
    )
    .await?;
    let png = journey.driver.screenshot_as_png().await?;
    cdp.send_raw("Emulation.clearDeviceMetricsOverride", json!({}))
        .await?;
    Ok(png)
}

/// The contrast rows (text roles, status colors, gradient stops) under AA: `text on background ratio`.
async fn contrast_failures(journey: &Journey) -> anyhow::Result<Vec<String>> {
    let measured = journey.count("[data-testid=contrast]").await?;
    anyhow::ensure!(measured > 0, "the gallery measured no contrast");
    let value = journey
        .eval(
            "return [...document.querySelectorAll('[data-testid=contrast][data-pass=false]')]
               .map((row) => `${row.dataset.text} on ${row.dataset.background} ${row.dataset.ratio}`);",
            Vec::new(),
        )
        .await?;
    Ok(serde_json::from_value(value)?)
}

// spec: UI-173, UI-174, UI-124, UI-135, UI-140, UI-145
#[tokio::test(flavor = "multi_thread")]
async fn the_kit_gallery_text_reaches_aa_in_every_theme() -> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    let mut problems = Vec::new();
    for theme in THEMES {
        for scheme in SCHEMES {
            open(&journey, theme, scheme).await?;
            assert_eq!(
                journey.attr("html", "data-motion").await?.as_deref(),
                Some("none"),
                "the gallery runs at motion none"
            );
            for failure in contrast_failures(&journey).await? {
                problems.push(format!("{theme} {scheme}: under AA: {failure}"));
            }
            let path = shot_path(theme, scheme);
            std::fs::write(&path, full_page(&journey).await?)?;
            eprintln!("screenshot: {}", path.display());
        }
    }
    journey.finish("kit gallery").await?;
    if !problems.is_empty() {
        bail!("the gallery's contrast:\n{}", problems.join("\n"));
    }
    Ok(())
}
