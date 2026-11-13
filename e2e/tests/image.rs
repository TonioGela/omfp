//! Images in `omfp:file` (E2E-53), against a daemon on the fake launcher (and its fake git): the workspace folder is a
//! repository whose 9 MB BMP (over the daemon's 8 MiB base64 cap) is blue at `HEAD` and changed in the working copy.
//! It shows from its raw bytes (DMN-250), fitted, zooms and pans, and compares with its `HEAD` version (`?rev=HEAD`,
//! DMN-255) side by side and under a slider (shots `omfp-image-fit.png`, `omfp-image-split.png`,
//! `omfp-image-slider.png`).

use e2e::Journey;
use omfp_daemon::agent::FakeLauncher;
use serde_json::{Value, json};

const VIEW: &str = "[data-testid=file] [data-testid=file-image]";
const AFTER: &str = "[data-testid=file-image] img[data-testid=image-after]";
const BEFORE: &str = "[data-testid=file-image] img[data-testid=image-before]";
const VIEWPORT: &str = "[data-testid=file-image] [data-testid=image-viewport]";
const ZOOM: &str = "[data-testid=file-image] [data-testid=image-zoom]";

const WIDTH: u32 = 2000;
const HEIGHT: u32 = 1500;

/// A 24-bit BMP of `WIDTH` × `HEIGHT` pixels: blue, its right half red when `red_right` (9 MB, no compression).
fn bmp(red_right: bool) -> Vec<u8> {
    let row = WIDTH * 3;
    let data = row * HEIGHT;
    let mut bytes = Vec::with_capacity(54 + data as usize);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&(54 + data).to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&54_u32.to_le_bytes());
    bytes.extend_from_slice(&40_u32.to_le_bytes());
    bytes.extend_from_slice(&WIDTH.to_le_bytes());
    bytes.extend_from_slice(&HEIGHT.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&24_u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 24]);
    for _ in 0..HEIGHT {
        for x in 0..WIDTH {
            // Blue, green, red.
            let pixel = if red_right && x >= WIDTH / 2 {
                [0x20, 0x20, 0xe0]
            } else {
                [0xe0, 0x50, 0x20]
            };
            bytes.extend_from_slice(&pixel);
        }
    }
    bytes
}

/// `[loaded, natural width, rendered width, red share of the right half, src]` of the image `css`: its pixels drawn
/// on a canvas of 200 × 150 (the image is same-origin).
async fn image_state(journey: &Journey, css: &str) -> anyhow::Result<Value> {
    journey
        .eval(
            "const img = document.querySelector(arguments[0]);
             if (!img) return null;
             if (!img.complete || img.naturalWidth === 0) return [false, 0, 0, 0, img.getAttribute('src')];
             const canvas = document.createElement('canvas');
             canvas.width = 200; canvas.height = 150;
             const context = canvas.getContext('2d');
             context.drawImage(img, 0, 0, 200, 150);
             const data = context.getImageData(100, 0, 100, 150).data;
             let red = 0;
             for (let i = 0; i < data.length; i += 4) { if (data[i] > 150 && data[i + 2] < 100) red++; }
             return [img.dataset.loaded === 'true', img.naturalWidth, img.getBoundingClientRect().width,
                     red / (100 * 150), img.getAttribute('src')];",
            vec![json!(css)],
        )
        .await
}

async fn until_loaded(journey: &Journey, css: &str) -> anyhow::Result<Value> {
    journey
        .until(&format!("{css} to load"), async |page| {
            let state = image_state(page, css).await?;
            Ok((state[0] == json!(true)).then_some(state))
        })
        .await
}

async fn zoom_label(journey: &Journey) -> anyhow::Result<u32> {
    let text = journey.text(ZOOM).await?;
    Ok(text.trim_end_matches('%').parse()?)
}

/// Zooms, pans and Ctrl-wheels the working copy alone (UI-549..UI-551).
async fn zooms_and_pans(journey: &Journey, fitted: u32) -> anyhow::Result<()> {
    journey.click("[data-testid=image-zoom-in]").await?;
    let zoomed = zoom_label(journey).await?;
    assert!(zoomed > fitted, "zoomed in: {fitted}% → {zoomed}%");
    journey.click("[data-testid=image-actual]").await?;
    assert_eq!(journey.text(ZOOM).await?, "100%");
    let actual = image_state(journey, AFTER).await?;
    assert!(
        (actual[2].as_f64().unwrap_or_default() - f64::from(WIDTH)).abs() < 1.0,
        "its own pixels: {actual}"
    );
    // A drag to the left pans right.
    let panned = journey
        .eval(
            "const viewport = document.querySelector(arguments[0]);
             const box = viewport.getBoundingClientRect();
             const at = (type, x) => viewport.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: 7,
               button: 0, clientX: box.left + x, clientY: box.top + 100 }));
             const before = viewport.scrollLeft;
             at('pointerdown', 300); at('pointermove', 180); at('pointerup', 180);
             return viewport.scrollLeft - before;",
            vec![json!(VIEWPORT)],
        )
        .await?;
    assert_eq!(panned, json!(120), "dragged 120 px");
    // Ctrl with the wheel zooms the image, not the page.
    let prevented = journey
        .eval(
            "const viewport = document.querySelector(arguments[0]);
             const box = viewport.getBoundingClientRect();
             const wheel = new WheelEvent('wheel', { bubbles: true, cancelable: true, ctrlKey: true, deltaY: 500,
               clientX: box.left + 100, clientY: box.top + 100 });
             viewport.dispatchEvent(wheel);
             return wheel.defaultPrevented;",
            vec![json!(VIEWPORT)],
        )
        .await?;
    assert_eq!(prevented, json!(true));
    journey
        .until("the wheel's zoom", async |page| {
            Ok((zoom_label(page).await? == 37).then_some(()))
        })
        .await
}

/// Compares with `HEAD` side by side, then under a slider (UI-553..UI-555).
async fn compares_with_head(journey: &Journey) -> anyhow::Result<()> {
    // Fitted again, the comparison fits both versions in their panes.
    journey.click("[data-testid=image-fit]").await?;
    journey.click("[data-testid=file-diff]").await?;
    let head = until_loaded(journey, BEFORE).await?;
    let src = head[4].as_str().unwrap_or_default();
    assert!(
        src.starts_with("/files/") && src.contains("/art/photo.bmp?rev=HEAD&v="),
        "HEAD's URL: {src}"
    );
    assert!(
        head[3].as_f64().unwrap_or(1.0) < 0.01,
        "HEAD is all blue: {head}"
    );
    let now = image_state(journey, AFTER).await?;
    assert!(
        now[3].as_f64().unwrap_or_default() > 0.99,
        "the working copy's right half is red: {now}"
    );
    assert_eq!(
        journey.attrs(VIEWPORT, "data-side").await?,
        vec![Some("before".to_owned()), Some("after".to_owned())]
    );
    journey.fits(VIEW).await?;
    journey.screenshot("image-split").await?;

    journey.click("[data-testid=file-layout]").await?;
    journey.element("[data-testid=image-divider]").await?;
    let moved = journey
        .eval(
            "const slider = document.querySelector('[data-testid=image-slider]');
             const divider = () => document.querySelector('[data-testid=image-divider]').style.left;
             const first = divider();
             Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(slider, '25');
             slider.dispatchEvent(new Event('input', { bubbles: true }));
             return [first, divider(), document.querySelector(arguments[0]).style.clipPath];",
            vec![json!(AFTER)],
        )
        .await?;
    let px = |value: &Value| {
        value
            .as_str()
            .unwrap_or_default()
            .trim_end_matches("px")
            .parse::<f64>()
            .unwrap_or_default()
    };
    assert!(px(&moved[1]) < px(&moved[0]), "the cut moved left: {moved}");
    assert!(
        moved[2].as_str().unwrap_or_default().starts_with("inset("),
        "the working copy is cut: {moved}"
    );
    journey.screenshot("image-slider").await?;
    Ok(())
}

// spec: E2E-53, UI-548, UI-549, UI-550, UI-551, UI-553, UI-554, UI-555, DMN-250, DMN-255
#[tokio::test(flavor = "multi_thread")]
async fn an_image_changed_since_head_zooms_pans_and_compares() -> anyhow::Result<()> {
    let journey = Journey::start_fake(FakeLauncher::new()).await?;
    journey.connected().await?;
    let thread = journey.create_thread("images").await?;
    let folder = journey.workspace_folder(&thread).await?;
    std::fs::create_dir_all(folder.join("art"))?;
    // The workspace folder is a repository whose `HEAD` has the photo blue; the working copy is half red.
    let git = journey
        .git
        .clone()
        .ok_or_else(|| anyhow::anyhow!("a fake launcher journey has the fake git"))?;
    git.add_repo(&folder, "main");
    git.commit_file(&folder, "art/photo.bmp", &bmp(false));
    std::fs::write(folder.join("art/photo.bmp"), bmp(true))?;
    // A rescan the daemon has finished: the folder's repository holds the image before it opens.
    journey.workspace_folder(&thread).await?;

    journey.command("open art/photo.bmp").await?;
    let shown = until_loaded(&journey, AFTER).await?;
    let src = shown[4].as_str().unwrap_or_default();
    assert!(
        src.starts_with("/files/") && src.contains("/art/photo.bmp?v="),
        "over the cap, from the raw bytes: {src}"
    );
    assert_eq!(shown[1], json!(WIDTH));
    assert_eq!(
        journey
            .attr("[data-testid=image-fit]", "aria-pressed")
            .await?,
        Some("true".to_owned())
    );
    let fitted = zoom_label(&journey).await?;
    assert!(fitted < 100, "fitted below 100%: {fitted}%");
    journey.fits(VIEWPORT).await?;
    journey.screenshot("image-fit").await?;

    zooms_and_pans(&journey, fitted).await?;
    compares_with_head(&journey).await?;
    journey.finish("E2E-53").await
}
