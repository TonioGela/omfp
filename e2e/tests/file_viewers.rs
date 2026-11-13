//! The viewers of `omfp:file` (06-testing.md, E2E-57): a workspace file opened with `:open` shows its rendered form
//! (a Markdown preview), its relative links open the files they name, and the toggle shows the source. The journeys run
//! on the fake launcher and take screenshots (`omfp-file-viewer-*.png` in `/tmp` or `OMFP_SCREENSHOT_DIR`).

use e2e::Journey;
use omfp_daemon::agent::FakeLauncher;
use omfp_testkit::TempRepo;

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
