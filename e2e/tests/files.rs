//! The files journeys of M5c (06-testing.md, E2E-13): a workspace file opened from a tool card, edited with vim keys and
//! saved, reloaded when the agent rewrites it, a stale save shown as a conflict, and a changed file opened against
//! `HEAD` from `omfp:changes`; bare file names opened directly or through the picker, and the open files as tabs of
//! the floating panel in a view without `main`. They also take the M5c screenshots (`omfp-m5c-*.png` in `/tmp` or
//! `OMFP_SCREENSHOT_DIR`). Run with `just test-e2e`.

use e2e::Journey;
use fake_llm::{Reply, Step, ToolCall};
use omfp_testkit::TempRepo;
use serde_json::json;
use thirtyfour::prelude::*;

const EDITOR: &str = "[data-testid=file] [data-testid=file-editor]";

/// `path` without its last three characters: typed into a path field, its completions include `path`.
fn partial(path: &std::path::Path) -> String {
    let path = path.to_string_lossy();
    path[..path.len() - 3].to_owned()
}

/// The text the open file's editor shows.
async fn editor_text(journey: &Journey) -> anyhow::Result<String> {
    let value = journey
        .eval(
            "const e = document.querySelector(arguments[0]);
             return e ? [...e.querySelectorAll('.cm-line')].map(l => l.textContent).join('\\n') : null;",
            vec![json!(format!("{EDITOR} .cm-content"))],
        )
        .await?;
    Ok(value.as_str().unwrap_or_default().to_owned())
}

/// Waits until the open file's editor shows `text`.
async fn until_editor_text(journey: &Journey, text: &str) -> anyhow::Result<()> {
    journey
        .until(&format!("the editor to show {text:?}"), async |page| {
            Ok((editor_text(page).await? == text).then_some(()))
        })
        .await
}

/// Waits until the file editor is in `mode` (`view`, `edit`, `conflict`).
async fn until_mode(journey: &Journey, mode: &str) -> anyhow::Result<()> {
    journey
        .until(&format!("the file in {mode} mode"), async |page| {
            Ok((page.attr(EDITOR, "data-mode").await?.as_deref() == Some(mode)).then_some(()))
        })
        .await
}

/// Waits until the mode line shows vim's `mode` (UI-112).
async fn until_vim(journey: &Journey, mode: &str) -> anyhow::Result<()> {
    journey
        .until(
            &format!("vim's {mode} mode on the mode line"),
            async |page| {
                let vim = page.attr("[data-testid=mode-line]", "data-vim").await?;
                Ok((vim.as_deref() == Some(mode)).then_some(()))
            },
        )
        .await
}

/// Focuses the read-only editor and presses `i`: the file switches to editing, in vim's insert mode (UI-111).
async fn start_editing(journey: &Journey) -> anyhow::Result<()> {
    journey.click(&format!("{EDITOR} .cm-content")).await?;
    journey.keys("i").await?;
    until_mode(journey, "edit").await?;
    until_vim(journey, "insert").await
}

/// Leaves insert mode and runs vim's ex command `command` (`w`, `q`).
async fn ex(journey: &Journey, command: &str) -> anyhow::Result<()> {
    journey.keys(Key::Escape).await?;
    until_vim(journey, "normal").await?;
    journey.keys(format!(":{command}")).await?;
    journey.keys(Key::Enter).await
}

async fn until_disk(journey: &Journey, file: &std::path::Path, text: &str) -> anyhow::Result<()> {
    journey
        .until(
            &format!("{} to hold {text:?}", file.display()),
            async |_| Ok((std::fs::read_to_string(file)? == text).then_some(())),
        )
        .await
}

/// The path of the `read` card opens the file in a tab next to the chat, read-only (UI-115, UI-116, UI-110).
async fn opens_from_the_read_card(journey: &Journey) -> anyhow::Result<()> {
    let read = "[data-testid=tool-card][data-tool=read][data-status=succeeded]";
    journey.element(read).await?;
    let toggle = format!("{read} [aria-expanded]");
    let collapsed = journey.attr(&toggle, "aria-expanded").await?;
    journey
        .click(&format!("{read} [data-testid=tool-path-open]"))
        .await?;
    journey
        .element("[data-slot=main] [data-testid=file-tab][aria-selected=true]")
        .await?;
    let tabs = journey
        .attrs("[data-slot=main] [role=tab]", "data-testid")
        .await?;
    assert_eq!(
        tabs,
        vec![Some("view-tab".to_owned()), Some("file-tab".to_owned())],
        "the file's tab follows the chat's"
    );
    assert_eq!(
        journey
            .text("[data-slot=main] [data-testid=file-tab]")
            .await?,
        "notes.txt"
    );
    // The chat's tab still holds the card, which the click did not toggle.
    journey
        .click("[data-slot=main] [data-testid=view-tab]")
        .await?;
    journey.element(read).await?;
    assert_eq!(
        journey.attr(&toggle, "aria-expanded").await?,
        collapsed,
        "opening the path does not toggle the card"
    );
    journey
        .click("[data-slot=main] [data-testid=file-tab]")
        .await?;
    until_mode(journey, "view").await?;
    until_editor_text(journey, "alpha\nbeta\n").await?;
    journey.screenshot("m5c-file").await?;
    Ok(())
}

/// The file changes on disk while the user edits it: the save is refused and shows the conflict view, and nothing is
/// written until the user decides (UI-113, DMN-090).
async fn conflicts_with_a_change_on_disk(
    journey: &Journey,
    disk: &std::path::Path,
) -> anyhow::Result<()> {
    start_editing(journey).await?;
    journey.keys("mine ").await?;
    let theirs = "alpha\nchanged on disk\n";
    std::fs::write(disk, theirs)?;
    journey
        .element("[data-testid=file] [data-testid=file-changed]")
        .await?;
    ex(journey, "w").await?;
    journey
        .element("[data-testid=file] [data-testid=file-conflict] [data-testid=file-conflict-diff] .cm-mergeView")
        .await?;
    assert_eq!(
        std::fs::read_to_string(disk)?,
        theirs,
        "a refused save writes nothing"
    );
    journey.screenshot("m5c-conflict").await?;
    journey.click("[data-testid=file-discard]").await?;
    until_mode(journey, "view").await?;
    until_editor_text(journey, theirs).await?;
    assert_eq!(std::fs::read_to_string(disk)?, theirs);
    Ok(())
}

/// The changed file's path in `omfp:changes` opens it against `HEAD` in the unified merge view (UI-114, UI-116).
async fn opens_against_head_from_changes(journey: &Journey) -> anyhow::Result<()> {
    let clicked = journey
        .eval(
            "const tab = [...document.querySelectorAll('[data-testid=view-tab]')]
               .find(t => t.textContent === 'Changes');
             if (tab) { tab.click(); }
             return tab !== undefined;",
            Vec::new(),
        )
        .await?;
    assert_eq!(clicked, json!(true), "the default view has a Changes tab");
    let entry = "[data-testid=changes-file][data-path=\"notes.txt\"]";
    journey
        .click(&format!("{entry} [data-testid=changes-open]"))
        .await?;
    let diff = "[data-testid=file] [data-testid=file-head-diff]";
    journey
        .element(&format!("{diff} .cm-content .cm-changedLine"))
        .await?;
    let deleted = journey
        .texts(&format!("{diff} .cm-deletedChunk"))
        .await?
        .join("\n");
    assert!(
        deleted.contains("beta"),
        "HEAD's line shows deleted: {deleted:?}"
    );
    let added = journey
        .texts(&format!("{diff} .cm-changedLine"))
        .await?
        .join("\n");
    assert!(
        added.contains("changed on disk"),
        "the new line shows inserted: {added:?}"
    );
    assert_eq!(
        journey
            .count("[data-slot=main] [data-testid=file-tab]")
            .await?,
        1,
        "the open file's tab is reused"
    );
    journey.screenshot("m5c-diff").await?;
    Ok(())
}

// spec: E2E-13, UI-110, UI-111, UI-112, UI-113, UI-114, UI-115, UI-116, DMN-089, DMN-090
#[tokio::test(flavor = "multi_thread")]
async fn a_file_opened_from_a_tool_card_is_edited_reloaded_conflicted_and_diffed()
-> anyhow::Result<()> {
    let repo = TempRepo::new()?;
    repo.commit_file("notes.txt", "alpha\nbeta\n", "add notes")?;
    let name = repo
        .path()
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let file = format!("{name}/notes.txt");
    let disk = repo.path().join("notes.txt");
    let rewritten = "alpha\nbeta\nrewritten by the agent\n";
    let journey = Journey::start(vec![
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_read_1",
            "read",
            json!({ "path": file }),
        )])),
        // Held until the user has edited and saved the file and left editing.
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_write_1",
            "write",
            json!({ "path": file, "content": rewritten }),
        )]))
        .gated(),
        Step::reply(Reply::text("Rewrote the notes.")),
    ])
    .await?;
    journey.connected().await?;
    journey
        .new_linked_workspace(&partial(repo.path()), &repo.path().to_string_lossy())
        .await?;
    let thread = journey.first_message("look at the notes").await?;

    opens_from_the_read_card(&journey).await?;

    // `i`, an edit and `:w` write the file; `:q` makes it read-only again (UI-111, UI-112, DMN-090).
    start_editing(&journey).await?;
    journey.keys(Key::Control + Key::End).await?;
    journey.keys("gamma").await?;
    journey
        .until("the unsaved edit", async |page| {
            Ok((page.attr(EDITOR, "data-dirty").await?.as_deref() == Some("true")).then_some(()))
        })
        .await?;
    journey.screenshot("m5c-editing").await?;
    ex(&journey, "w").await?;
    until_disk(&journey, &disk, "alpha\nbeta\ngamma").await?;
    journey
        .until("the saved edit", async |page| {
            Ok((page.attr(EDITOR, "data-dirty").await?.as_deref() == Some("false")).then_some(()))
        })
        .await?;
    journey.keys(":q").await?;
    journey.keys(Key::Enter).await?;
    until_mode(&journey, "view").await?;

    // While the tab is read-only the agent rewrites the file: the view follows it (UI-110).
    journey.fake.release_gate(1);
    // The chat's tab is hidden behind the file's: the thread list tells when the turn ended.
    journey.status(&thread, "idle").await?;
    until_disk(&journey, &disk, rewritten).await?;
    until_editor_text(&journey, rewritten).await?;

    conflicts_with_a_change_on_disk(&journey, &disk).await?;

    opens_against_head_from_changes(&journey).await?;
    journey.finish("E2E-13").await
}

/// The `data-file` of the file tabs in `css` (the `main` slot's or the floating panel's).
async fn file_tabs(journey: &Journey, css: &str) -> anyhow::Result<Vec<String>> {
    Ok(journey
        .attrs(css, "data-file")
        .await?
        .into_iter()
        .flatten()
        .collect())
}

// spec: E2E-13, UI-115, UI-117, DMN-092
#[tokio::test(flavor = "multi_thread")]
async fn bare_names_open_their_file_or_a_picker_and_files_float_as_tabs_without_main()
-> anyhow::Result<()> {
    let repo = TempRepo::new()?;
    repo.commit_file("src/theme.ts", "export const theme = 1;\n", "theme")?;
    repo.commit_file("api/config.ts", "export const api = 1;\n", "api config")?;
    repo.commit_file("web/config.ts", "export const web = 1;\n", "web config")?;
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    let thread = journey.create_thread("bare names").await?;
    let folder = journey.workspace_folder(&thread).await?;
    std::os::unix::fs::symlink(repo.path(), folder.join("app"))?;
    // Rescans the workspace: `app` is one of its repositories now.
    journey.workspace_folder(&thread).await?;

    // One file has that name: it opens directly, in place of the bare name.
    journey.command("open theme.ts").await?;
    journey
        .element("[data-slot=main] [data-testid=file-tab][data-file=\"app/src/theme.ts\"][aria-selected=true]")
        .await?;
    until_editor_text(&journey, "export const theme = 1;\n").await?;

    // Two have it: the picker offers both, the arrows and Enter choose one.
    journey.command("open config.ts").await?;
    journey.until_focused("file-picker-input").await?;
    assert_eq!(
        journey
            .attrs("[data-testid=file-picker-option]", "data-path")
            .await?,
        vec![
            Some("app/api/config.ts".to_owned()),
            Some("app/web/config.ts".to_owned())
        ]
    );
    journey.screenshot("m5c-file-picker").await?;
    journey.keys(Key::Down).await?;
    journey.keys(Key::Enter).await?;
    journey.gone("[data-testid=file-picker]").await?;
    journey
        .element("[data-slot=main] [data-testid=file-tab][data-file=\"app/web/config.ts\"][aria-selected=true]")
        .await?;
    until_editor_text(&journey, "export const web = 1;\n").await?;
    assert_eq!(
        file_tabs(&journey, "[data-slot=main] [data-testid=file-tab]").await?,
        ["app/src/theme.ts", "app/web/config.ts"],
        "the chosen file took the bare name's tab"
    );

    // A view without `main`: both files are tabs of the floating panel.
    let manifest = json!({
        "default": "main",
        "views": {
            "main": { "layout": ["row", {}, ["tabs", { "id": "main" }, "omfp:chat"], "omfp:workspace"] },
            "plain": { "title": "Plain", "layout": ["row", {}, "omfp:chat", "omfp:workspace"] }
        }
    });
    std::fs::write(
        folder.join(".omfp/views.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    journey
        .element("[data-testid=view][data-view=main]")
        .await?;
    journey.command("view plain").await?;
    journey
        .element("[data-testid=view][data-view=plain]")
        .await?;
    let panel = "[data-testid=floating-file]";
    assert_eq!(
        file_tabs(
            &journey,
            &format!("{panel} [data-testid=floating-file-tab]")
        )
        .await?,
        ["app/src/theme.ts", "app/web/config.ts"]
    );
    until_editor_text(&journey, "export const web = 1;\n").await?;
    journey
        .click(&format!(
            "{panel} [data-testid=floating-file-tab][data-file=\"app/src/theme.ts\"]"
        ))
        .await?;
    until_editor_text(&journey, "export const theme = 1;\n").await?;
    journey.screenshot("m5c-floating-files").await?;
    journey.finish("E2E-13-names").await
}
