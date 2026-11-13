//! The viewers of `omfp:file` (06-testing.md, E2E-57, E2E-58): a workspace file opened with `:open` shows its rendered
//! form (a Markdown preview, a CSV table), its relative links open the files they name, its table sorts and scrolls
//! under a sticky header, and the toggle shows the source. The journeys run on the fake launcher and take screenshots
//! (`omfp-file-viewer-*.png` in `/tmp` or `OMFP_SCREENSHOT_DIR`).

use e2e::Journey;
use omfp_daemon::agent::FakeLauncher;
use omfp_testkit::TempRepo;
use serde_json::json;
use std::fmt::Write as _;

/// The selected tab's file view in the `main` slot.
const FILE: &str = "[data-slot=main] [role=tabpanel] [data-testid=file]";

/// A window on the fake launcher whose thread's workspace holds the repository `repo` as `app`.
async fn workspace_with(repo: &TempRepo) -> anyhow::Result<Journey> {
    let journey = Journey::start_fake(FakeLauncher::new()).await?;
    journey.connected().await?;
    let thread = journey.create_thread("viewers").await?;
    let folder = journey.workspace_folder(&thread).await?;
    std::os::unix::fs::symlink(repo.path(), folder.join("app"))?;
    // Rescans the workspace: `app` is one of its repositories now.
    journey.workspace_folder(&thread).await?;
    Ok(journey)
}

// spec: E2E-57, UI-560, UI-561
#[tokio::test(flavor = "multi_thread")]
async fn a_markdown_file_shows_its_preview_follows_its_links_and_toggles_to_the_source()
-> anyhow::Result<()> {
    let repo = TempRepo::new()?;
    repo.commit_file(
        "docs/guide.md",
        "# The guide\n\nSome *emphasis*, a list:\n\n- one\n- two\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n\
         Read [the next part](next.md) next.\n",
        "guide",
    )?;
    repo.commit_file("docs/next.md", "# Next part\n", "next")?;
    let journey = workspace_with(&repo).await?;

    journey.command("open app/docs/guide.md").await?;
    let preview = format!(
        "{FILE}[data-path=\"app/docs/guide.md\"] [data-testid=file-viewer][data-viewer=markdown]"
    );
    journey.element(&format!("{preview} h1")).await?;
    assert_eq!(journey.text(&format!("{preview} h1")).await?, "The guide");
    assert_eq!(journey.count(&format!("{preview} table td")).await?, 2);
    assert_eq!(
        journey
            .count(&format!("{FILE} [data-testid=file-editor]"))
            .await?,
        0
    );
    journey.screenshot("file-viewer-markdown").await?;

    // The relative link opens the file next to it, in its own tab.
    journey
        .click(&format!("{preview} [data-testid=path-link]"))
        .await?;
    let next = "[data-slot=main] [data-testid=file-tab][data-file=\"app/docs/next.md\"][aria-selected=true]";
    journey.element(next).await?;
    let next_heading =
        format!("{FILE}[data-path=\"app/docs/next.md\"] [data-testid=file-viewer] h1");
    journey.element(&next_heading).await?;
    assert_eq!(journey.text(&next_heading).await?, "Next part");

    // Back on the guide, the toggle shows the source in the editor.
    journey
        .click("[data-slot=main] [data-testid=file-tab][data-file=\"app/docs/guide.md\"]")
        .await?;
    journey
        .click(&format!("{FILE} [data-testid=file-view-toggle]"))
        .await?;
    journey.gone(&preview).await?;
    journey
        .until("the source in the editor", async |page| {
            let first = page
                .texts(&format!("{FILE} [data-testid=file-editor] .cm-line"))
                .await?;
            Ok((first.first().map(String::as_str) == Some("# The guide")).then_some(()))
        })
        .await?;
    journey.screenshot("file-viewer-markdown-source").await?;
    Ok(())
}

// spec: E2E-58, UI-563, UI-564, UI-565
#[tokio::test(flavor = "multi_thread")]
async fn a_csv_file_too_large_to_read_whole_shows_as_a_sorted_virtual_table_under_a_sticky_header()
-> anyhow::Result<()> {
    let repo = TempRepo::new()?;
    // About 2.3 MB: more than the daemon reads (DMN-089), so the table shows the first rows and says so.
    let mut csv = String::from("name,count,note\n");
    for index in 0..40_000 {
        writeln!(
            csv,
            "item {index:05},{},\"a note, with a comma, padding the row out\"",
            (index * 7919) % 1000
        )?;
    }
    std::fs::create_dir_all(repo.path().join("data"))?;
    std::fs::write(repo.path().join("data/items.csv"), csv)?;
    let journey = workspace_with(&repo).await?;

    journey.command("open app/data/items.csv").await?;
    let table =
        format!("{FILE} [data-testid=file-viewer][data-viewer=table] [data-testid=file-table]");
    journey
        .element(&format!("{table} tbody tr[data-index]"))
        .await?;
    assert_eq!(
        journey.texts(&format!("{table} thead th")).await?,
        ["name", "count", "note"]
    );
    let summary = journey
        .text(&format!("{FILE} [data-testid=file-table-summary]"))
        .await?;
    assert!(
        summary.ends_with("MiB file, which is too large to read whole"),
        "the table says the file was cut: {summary}"
    );
    let rendered = journey
        .count(&format!("{table} tbody tr[data-index]"))
        .await?;
    assert!(rendered > 10 && rendered < 200, "{rendered} rows rendered");

    // Sorted by count, descending on the second click: the 999s come first.
    let count_header = format!("{table} thead th:nth-child(2) [data-testid=file-table-sort]");
    journey.click(&count_header).await?;
    journey.click(&count_header).await?;
    assert_eq!(
        journey
            .attr(&format!("{table} thead th:nth-child(2)"), "aria-sort")
            .await?
            .as_deref(),
        Some("descending")
    );
    journey
        .until("the 999s first", async |page| {
            let first = page
                .text(&format!("{table} tbody tr[data-index='0'] td:nth-child(2)"))
                .await?;
            Ok((first == "999").then_some(()))
        })
        .await?;

    // Scrolled far down, the header stays at the top of the box and the rows in view are rendered, few of them.
    journey
        .eval(
            "document.querySelector(arguments[0]).scrollTop = 20000 * 28;",
            vec![json!(table)],
        )
        .await?;
    journey
        .until("the rows around row 20000", async |page| {
            let indexes = page
                .attrs(&format!("{table} tbody tr[data-index]"), "data-index")
                .await?;
            Ok(indexes
                .iter()
                .flatten()
                .any(|index| index == "20000")
                .then_some(()))
        })
        .await?;
    let offset = journey
        .eval(
            "const box = document.querySelector(arguments[0]);
             return box.querySelector('thead th').getBoundingClientRect().top - box.getBoundingClientRect().top;",
            vec![json!(table)],
        )
        .await?;
    assert!(
        offset.as_f64().is_some_and(|offset| offset.abs() < 1.0),
        "the header sticks to the top: {offset}"
    );
    assert!(
        journey
            .count(&format!("{table} tbody tr[data-index]"))
            .await?
            < 200
    );
    journey.screenshot("file-viewer-table").await?;

    journey
        .click(&format!("{FILE} [data-testid=file-view-toggle]"))
        .await?;
    journey.gone(&table).await?;
    journey
        .element(&format!("{FILE} [data-testid=file-editor] .cm-content"))
        .await?;
    Ok(())
}
