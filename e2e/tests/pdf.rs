//! PDF documents in `omfp:file` (E2E-52), against a daemon on the fake launcher: a two-page PDF written by the test
//! opens in the kit's pdf.js viewer from its raw bytes (DMN-250) with the worker of the bundle (`/pdfjs/`), its first
//! page drawn on a canvas with its text selectable over it, the page indicator following the scroll, and the zoom
//! (shot `omfp-pdf.png`).

use e2e::Journey;
use omfp_daemon::agent::FakeLauncher;
use serde_json::{Value, json};
use std::io::Write as _;

const VIEW: &str = "[data-testid=file] [data-testid=pdf-view]";
const FIRST_PAGE: &str = "[data-testid=pdf-page][data-page='1']";
const PAGES: &str = "[data-testid=pdf-pages]";
const INDICATOR: &str = "[data-testid=pdf-page-number]";

/// A PDF of US Letter pages, one per text: each shows its text in Helvetica above a blue box. Written by hand, with
/// the cross-reference table's byte offsets counted as the objects are written.
fn pdf(texts: &[&str]) -> std::io::Result<Vec<u8>> {
    let pages = texts.len();
    // Objects: 1 catalog, 2 page tree, 3 font, then a page and its content stream per text.
    let kids = (0..pages)
        .map(|index| format!("{} 0 R", 4 + 2 * index))
        .collect::<Vec<_>>()
        .join(" ");
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        format!("<< /Type /Pages /Kids [{kids}] /Count {pages} >>"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];
    for (index, text) in texts.iter().enumerate() {
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> \
             /Contents {} 0 R >>",
            5 + 2 * index
        ));
        let stream =
            format!("BT /F1 36 Tf 72 680 Td ({text}) Tj ET\n0.2 0.4 0.8 rg 72 400 300 200 re f\n");
        objects.push(format!(
            "<< /Length {} >>\nstream\n{stream}endstream",
            stream.len()
        ));
    }
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        write!(bytes, "{} 0 obj\n{object}\nendobj\n", index + 1)?;
    }
    let xref = bytes.len();
    write!(
        bytes,
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    )?;
    for offset in offsets {
        writeln!(bytes, "{offset:010} 00000 n ")?;
    }
    write!(
        bytes,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    )?;
    Ok(bytes)
}

/// `[rendered, canvas width, blue pixels, text layer text, page width]` of the first page.
async fn first_page(journey: &Journey) -> anyhow::Result<Value> {
    journey
        .eval(
            "const page = document.querySelector(arguments[0]);
             if (!page) return null;
             const canvas = page.querySelector('canvas');
             let blue = 0;
             if (canvas.width > 0 && page.dataset.rendered === 'true') {
               const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
               for (let i = 0; i < data.length; i += 4) {
                 if (data[i + 2] > 150 && data[i] < 100) blue++;
               }
             }
             return [page.dataset.rendered === 'true', canvas.width, blue,
                     page.querySelector('.textLayer').textContent, page.getBoundingClientRect().width];",
            vec![json!(FIRST_PAGE)],
        )
        .await
}

// spec: E2E-52, UI-543, UI-545, UI-546, UI-547, DMN-250
#[tokio::test(flavor = "multi_thread")]
async fn a_pdf_file_shows_its_pages_in_the_viewer() -> anyhow::Result<()> {
    let journey = Journey::start_fake(FakeLauncher::new()).await?;
    journey.connected().await?;
    let thread = journey.create_thread("pdf").await?;
    let folder = journey.workspace_folder(&thread).await?;
    std::fs::create_dir_all(folder.join("docs"))?;
    let document = pdf(&["Hello PDF one", "Hello PDF two"])?;
    // All ASCII: the daemon reads it as text, the extension opens the viewer.
    assert!(document.is_ascii(), "an ASCII PDF");
    std::fs::write(folder.join("docs/guide.pdf"), document)?;

    journey.command("open docs/guide.pdf").await?;
    let url = journey
        .attr(VIEW, "data-url")
        .await?
        .ok_or_else(|| anyhow::anyhow!("no PDF view"))?;
    assert!(
        url.starts_with("/files/") && url.contains("/docs/guide.pdf?v="),
        "the raw file URL: {url}"
    );
    let page = journey
        .until("the first page to be drawn", async |page| {
            let state = first_page(page).await?;
            Ok((state[0] == json!(true)).then_some(state))
        })
        .await?;
    assert!(
        page[1].as_u64().unwrap_or_default() > 100,
        "a canvas: {page}"
    );
    assert!(
        page[2].as_u64().unwrap_or_default() > 1000,
        "the blue box drawn: {page}"
    );
    assert_eq!(page[3], json!("Hello PDF one"), "the text layer");
    assert_eq!(journey.text(INDICATOR).await?, "1 / 2");
    let worker = journey
        .eval(
            "return performance.getEntriesByType('resource').some(e => e.name.endsWith('/pdfjs/pdf.worker.min.mjs'));",
            Vec::new(),
        )
        .await?;
    assert_eq!(worker, json!(true), "the bundle's pdf.js worker");
    journey.fits(VIEW).await?;
    journey.screenshot("pdf").await?;

    // Scrolling to the end shows the second page in the indicator.
    journey
        .eval(
            "const pages = document.querySelector(arguments[0]); pages.scrollTop = pages.scrollHeight;",
            vec![json!(PAGES)],
        )
        .await?;
    journey
        .until("the indicator to say 2 / 2", async |page| {
            Ok((page.text(INDICATOR).await? == "2 / 2").then_some(()))
        })
        .await?;

    // Zooming in makes the pages wider than the fitted width.
    let fitted = page[4].as_f64().unwrap_or_default();
    journey.click("[data-testid=pdf-zoom-in]").await?;
    journey
        .until("the zoomed pages", async |page| {
            let state = first_page(page).await?;
            Ok((state[4].as_f64().unwrap_or_default() > fitted + 1.0).then_some(()))
        })
        .await?;
    assert_eq!(
        journey
            .attr("[data-testid=pdf-fit]", "aria-pressed")
            .await?,
        Some("false".to_owned())
    );
    journey.finish("E2E-52").await
}
