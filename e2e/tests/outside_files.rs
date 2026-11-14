//! A file outside the workspace folder (06-testing.md, E2E-70): the agent reads a file of a git worktree outside its
//! workspace by its absolute path; the read card's path opens it read-only with its note and its diff against that
//! worktree's `HEAD`, and `:open` opens another absolute path outside. Headless Chromium, omp for real on the fake
//! LLM; shots `omfp-outside-file*.png` in the run's screenshot directory.

use std::path::Path;

use e2e::Journey;
use fake_llm::{Reply, Step, ToolCall};
use omfp_testkit::TempRepo;
use serde_json::json;

/// The selected tab's file view in the `main` slot.
const FILE: &str = "[data-slot=main] [role=tabpanel] [data-testid=file]";

/// The note of a file outside the workspace (UI-700).
const NOTE: &str = "Outside the workspace: this file can be read here, not edited.";

/// `path` without its last three characters: typed into a path field, its completions include `path`.
fn partial(path: &Path) -> String {
    let path = path.to_string_lossy();
    path[..path.len() - 3].to_owned()
}

/// The view of the file `path` once the daemon read it as outside the workspace.
fn outside_view(path: &Path) -> String {
    format!(
        "{FILE}[data-path=\"{}\"][data-status=ready][data-outside=true]",
        path.display()
    )
}

/// The read card's absolute path opens the README previewed, read-only, with the note, its path and the chip of the
/// worktree holding it (UI-700, UI-702, DMN-310, DMN-311).
async fn opens_from_the_read_card(
    journey: &Journey,
    readme: &Path,
    worktree: &str,
) -> anyhow::Result<()> {
    let read = "[data-testid=tool-card][data-tool=read][data-status=succeeded]";
    journey
        .click(&format!("{read} [data-testid=tool-path-open]"))
        .await?;
    let view = outside_view(readme);
    let heading = format!("{view} [data-testid=file-viewer] h1");
    journey.element(&heading).await?;
    assert_eq!(journey.text(&heading).await?, "Elsewhere");
    assert_eq!(
        journey
            .text(&format!("{view} [data-testid=file-outside]"))
            .await?,
        NOTE
    );
    assert_eq!(
        journey
            .text(&format!("{view} [data-testid=file-path]"))
            .await?,
        readme.display().to_string()
    );
    let bar = journey
        .text(&format!("{view} [data-testid=file-bar]"))
        .await?;
    assert!(bar.contains(worktree), "{bar}");
    assert_eq!(
        journey
            .count(&format!("{view} [data-testid=file-edit]"))
            .await?,
        0
    );
    journey.screenshot("outside-file").await?;

    // Its diff is against `HEAD` of the worktree holding it (UI-704).
    journey
        .click(&format!("{view} [data-testid=file-diff]"))
        .await?;
    let changed = format!("{view} [data-testid=file-head-diff] .cm-changedLine");
    journey
        .until("the changed line against HEAD", async |page| {
            let lines = page.texts(&changed).await?;
            Ok(lines
                .iter()
                .any(|line| line.contains("changed by the agent"))
                .then_some(()))
        })
        .await?;
    journey.screenshot("outside-file-diff").await
}

/// `:open` with another absolute path outside opens it as typed, read-only in the editor, with the note (UI-701).
async fn opens_with_open(journey: &Journey, notes: &Path) -> anyhow::Result<()> {
    journey
        .command(&format!("open {}", notes.display()))
        .await?;
    let view = outside_view(notes);
    let editor = format!("{view} [data-testid=file-editor]");
    let lines = format!("{editor} .cm-line");
    journey
        .until("the untracked file in the editor", async |page| {
            let shown = page.texts(&lines).await?;
            Ok((shown.first().map(String::as_str) == Some("not tracked")).then_some(()))
        })
        .await?;
    assert_eq!(
        journey.attr(&editor, "data-read-only").await?.as_deref(),
        Some("true")
    );
    assert_eq!(
        journey
            .text(&format!("{view} [data-testid=file-outside]"))
            .await?,
        NOTE
    );
    journey.screenshot("outside-file-open").await
}

// spec: E2E-70, UI-700, UI-701, UI-702, UI-704, DMN-310, DMN-311
#[tokio::test(flavor = "multi_thread")]
async fn a_file_outside_the_workspace_opens_read_only_from_its_tool_card_and_from_open()
-> anyhow::Result<()> {
    let inside = TempRepo::new()?;
    let elsewhere = TempRepo::new()?;
    elsewhere.commit_file(
        "README.md",
        "# Elsewhere\n\nA worktree outside the workspace.\n",
        "readme",
    )?;
    let readme = elsewhere.path().join("README.md");
    std::fs::write(
        &readme,
        "# Elsewhere\n\nA worktree outside the workspace, changed by the agent.\n",
    )?;
    let notes = elsewhere.path().join("notes.txt");
    std::fs::write(&notes, "not tracked\n")?;
    let worktree = elsewhere
        .path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let journey = Journey::start(vec![
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_read_1",
            "read",
            json!({ "path": readme }),
        )])),
        Step::reply(Reply::text("Read the README of the other worktree.")),
    ])
    .await?;
    journey.connected().await?;
    journey
        .new_linked_workspace(&partial(inside.path()), &inside.path().to_string_lossy())
        .await?;
    let thread = journey.first_message("read the other README").await?;
    journey.status(&thread, "idle").await?;
    let folder = journey.workspace_folder(&thread).await?;
    assert!(!readme.starts_with(&folder), "{}", folder.display());

    opens_from_the_read_card(&journey, &readme, &worktree).await?;
    opens_with_open(&journey, &notes).await?;
    journey.finish("E2E-70").await
}
