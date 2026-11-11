//! Screenshot test of the `/kit` gallery (UI-173, UI-174): the whole page at motion `none`, 17 px, compact, in light and
//! dark for the built-in Synth palette and the matugen fixture theme, compared with the baselines checked in next to
//! this file (`kit-baselines/`), and the WCAG AA contrast the page measured for its text roles and gradient stops.
//! The screenshots land in `/tmp/omfp-kit-<theme>-<scheme>.png` (or `OMFP_SCREENSHOT_DIR`). After an intended visual
//! change, `OMFP_UPDATE_BASELINES=1 just test-e2e` rewrites the baselines: look at them before committing.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use e2e::Journey;
use serde_json::{Value, json};

/// The page width of every shot (CSS px, device scale 1).
const WIDTH: u32 = 1400;
/// A pixel differs when one channel moves more than this: rendering is deterministic, a theme tint is not noise.
const CHANNEL_TOLERANCE: u8 = 2;
/// Share of differing pixels a shot may have and still match its baseline.
const MAX_DIFFERING: f64 = 0.0005;

const THEMES: [&str; 2] = ["synth", "matugen-forest"];
const SCHEMES: [&str; 2] = ["light", "dark"];

fn baseline(theme: &str, scheme: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/kit-baselines")
        .join(format!("kit-{theme}-{scheme}.png"))
}

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

/// Decodes both PNG images in the page and counts the pixels that differ: `(width, height, differing)` of the shot, or an
/// error naming the size mismatch.
async fn differing_pixels(
    journey: &Journey,
    shot: &[u8],
    expected: &[u8],
) -> anyhow::Result<(u64, u64, u64)> {
    let result = journey
        .driver
        .execute_async(
            "const [shot, expected, tolerance, done] = arguments;
             const decode = async (bytes) => {
               const bitmap = await createImageBitmap(new Blob([new Uint8Array(bytes)], { type: 'image/png' }));
               const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
               const context = canvas.getContext('2d', { willReadFrequently: true });
               context.drawImage(bitmap, 0, 0);
               return context.getImageData(0, 0, bitmap.width, bitmap.height);
             };
             Promise.all([decode(shot), decode(expected)]).then(([a, b]) => {
               if (a.width !== b.width || a.height !== b.height) {
                 done({ error: `size ${a.width}x${a.height}, baseline ${b.width}x${b.height}` });
                 return;
               }
               let differing = 0;
               for (let i = 0; i < a.data.length; i += 4) {
                 if (Math.abs(a.data[i] - b.data[i]) > tolerance
                     || Math.abs(a.data[i + 1] - b.data[i + 1]) > tolerance
                     || Math.abs(a.data[i + 2] - b.data[i + 2]) > tolerance) {
                   differing += 1;
                 }
               }
               done({ width: a.width, height: a.height, differing });
             }, (error) => done({ error: String(error) }));",
            vec![json!(shot), json!(expected), json!(CHANNEL_TOLERANCE)],
        )
        .await?;
    let value: Value = result.json().clone();
    if let Some(error) = value["error"].as_str() {
        bail!("{error}");
    }
    let field = |name: &str| {
        value[name]
            .as_u64()
            .with_context(|| format!("{name} in {value}"))
    };
    Ok((field("width")?, field("height")?, field("differing")?))
}

/// The contrast rows of the checked pairs (text roles, gradient stops) under AA: `text on background ratio`.
async fn contrast_failures(journey: &Journey) -> anyhow::Result<Vec<String>> {
    let checked = journey
        .count("[data-testid=contrast][data-checked=true]")
        .await?;
    anyhow::ensure!(checked > 0, "the gallery measured no contrast");
    let value = journey
        .eval(
            "return [...document.querySelectorAll('[data-testid=contrast][data-checked=true][data-pass=false]')]
               .map((row) => `${row.dataset.text} on ${row.dataset.background} ${row.dataset.ratio}`);",
            Vec::new(),
        )
        .await?;
    Ok(serde_json::from_value(value)?)
}

// spec: UI-173, UI-174, UI-124, UI-135, UI-140, UI-145
#[tokio::test(flavor = "multi_thread")]
async fn the_kit_gallery_matches_its_baselines_and_its_text_reaches_aa_in_every_theme()
-> anyhow::Result<()> {
    let update = std::env::var_os("OMFP_UPDATE_BASELINES").is_some();
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
            let shot = full_page(&journey).await?;
            let path = shot_path(theme, scheme);
            std::fs::write(&path, &shot)?;
            eprintln!("screenshot: {}", path.display());
            let expected_path = baseline(theme, scheme);
            if update {
                std::fs::write(&expected_path, &shot)?;
                eprintln!("baseline updated: {}", expected_path.display());
                continue;
            }
            let expected = std::fs::read(&expected_path).with_context(|| {
                format!(
                    "no baseline {}: run with OMFP_UPDATE_BASELINES=1",
                    expected_path.display()
                )
            })?;
            match differing_pixels(&journey, &shot, &expected).await {
                Ok((width, height, differing)) => {
                    #[allow(clippy::cast_precision_loss)]
                    let share = differing as f64 / (width * height) as f64;
                    if share > MAX_DIFFERING {
                        problems.push(format!(
                            "{theme} {scheme}: {differing} pixels ({:.3} %) differ from {}; see {}",
                            share * 100.0,
                            expected_path.display(),
                            path.display()
                        ));
                    }
                }
                Err(error) => problems.push(format!(
                    "{theme} {scheme}: {error} ({} vs {})",
                    path.display(),
                    expected_path.display()
                )),
            }
        }
    }
    journey.finish("kit gallery screenshots").await?;
    if !problems.is_empty() {
        bail!("the gallery does not match:\n{}", problems.join("\n"));
    }
    Ok(())
}
