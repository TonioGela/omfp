//! The widget journeys of M4 (06-testing.md, E2E-10..12): an agent-made widget following a daemon job, views from
//! `views.json` with their error banner and switcher, and layout editing with the widget picker and a copied built-in.
//! They also take the M4 screenshots (`omfp-m4-*.png` in `/tmp` or `OMFP_SCREENSHOT_DIR`). Run with `just test-e2e`.

use std::path::Path;

use e2e::Journey;
use fake_llm::{Reply, Step, ToolCall};
use serde_json::{Value, json};

/// A job that prints `PASS <n>` once the gate file `.gate<n>` exists in the workspace folder, for n = 1..3: the
/// journey decides when each line comes, so nothing depends on timing.
const JOB: &str =
    "for i in 1 2 3; do while [ ! -e .gate$i ]; do sleep 0.05; done; echo PASS $i; done";

/// The widget the scripted agent writes: a progress bar over the job's `PASS` lines.
fn tests_widget(title: &str) -> String {
    format!(
        r#"import {{ Card, Log, Progress }} from "omfp/kit";
import {{ useJob }} from "omfp/sdk";

const COMMAND = {command};

export default function Tests() {{
  const job = useJob(COMMAND);
  const done = job.lines.filter((line: string) => line.startsWith("PASS")).length;
  return (
    <Card title={{job.running ? `{title} ${{done}}/3` : `{title} finished (exit ${{job.exitCode}})`}}>
      <div data-testid="tests" data-done={{done}} data-running={{String(job.running)}}>
        <Progress label="{title}" value={{done}} max={{3}} />
        <Log lines={{job.lines}} />
      </div>
    </Card>
  );
}}
"#,
        command = json!(JOB),
    )
}

/// The manifest the scripted agent writes: the default layout with the tests widget under the workspace widget.
fn agent_manifest() -> Value {
    json!({
        "$schema": "omfp://schemas/views.json",
        "default": "work",
        "views": {
            "work": {
                "title": "Work",
                "layout": ["row", { "sizes": [3, 1] },
                    ["col", {}, "omfp:header", "omfp:chat"],
                    ["col", {}, "omfp:workspace", "./widgets/tests"]]
            }
        }
    })
}

/// The `data-done` count the tests widget shows, once it shows one.
async fn tests_done(journey: &Journey, done: u32) -> anyhow::Result<()> {
    let css = "[data-testid=widget][data-widget=\"./widgets/tests\"] [data-testid=tests]";
    journey
        .until(&format!("the tests widget at {done}/3"), async |page| {
            Ok(
                (page.attr(css, "data-done").await?.as_deref() == Some(&done.to_string()))
                    .then_some(()),
            )
        })
        .await
}

fn open_gate(folder: &Path, n: u32) -> anyhow::Result<()> {
    std::fs::write(folder.join(format!(".gate{n}")), "")?;
    Ok(())
}

// spec: E2E-10, AGT-012, AGT-013, AGT-014, AGT-019, UI-105
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_widget_follows_a_daemon_job_live_and_recovers_from_a_syntax_error()
-> anyhow::Result<()> {
    let journey = Journey::start(vec![
        Step::reply(Reply::tool_calls(vec![
            ToolCall::new(
                "call_write_widget",
                "write",
                json!({ "path": ".omfp/widgets/tests/index.tsx", "content": tests_widget("Tests") }),
            ),
            ToolCall::new(
                "call_write_views",
                "write",
                json!({
                    "path": ".omfp/views.json",
                    "content": serde_json::to_string_pretty(&agent_manifest())?,
                }),
            ),
        ])),
        Step::reply(Reply::text("Added a tests widget.")),
    ])
    .await?;
    journey.connected().await?;
    let thread = journey.create_thread("widgets journey").await?;
    let folder = journey.workspace_folder(&thread).await?;
    journey
        .element("[data-testid=view][data-view=default] [data-widget=\"omfp:workspace\"]")
        .await?;
    journey.screenshot("m4-default-view").await?;

    // The agent writes the widget and the manifest: the widget appears live, its job started by the daemon.
    journey.send("add a tests widget").await?;
    journey.reply("Added a tests widget.").await?;
    journey
        .element("[data-testid=view][data-view=work]")
        .await?;
    tests_done(&journey, 0).await?;
    open_gate(&folder, 1)?;
    tests_done(&journey, 1).await?;
    journey.screenshot("m4-widget-mid-run").await?;
    open_gate(&folder, 2)?;
    tests_done(&journey, 2).await?;

    // A syntax error saved into the widget shows in its place with the file and line; the next save recovers, and
    // the widget follows the same job (its id is stored by the daemon), so the job is not started again.
    let widget = folder.join(".omfp/widgets/tests/index.tsx");
    let broken = tests_widget("Tests").replace("const COMMAND", "const = COMMAND");
    std::fs::write(&widget, broken)?;
    let error =
        "[data-widget=\"./widgets/tests\"] [data-testid=widget-error][data-error-kind=compile]";
    journey.element(error).await?;
    let shown = journey.text(error).await?;
    assert!(shown.contains("index.tsx:4"), "file and line: {shown}");
    std::fs::write(&widget, tests_widget("Tests (fixed)"))?;
    journey.gone(error).await?;
    tests_done(&journey, 2).await?;
    open_gate(&folder, 3)?;
    tests_done(&journey, 3).await?;
    journey
        .until("the job to finish", async |page| {
            let css = "[data-widget=\"./widgets/tests\"] [data-testid=tests]";
            Ok((page.attr(css, "data-running").await?.as_deref() == Some("false")).then_some(()))
        })
        .await?;
    journey.finish("E2E-10").await
}

fn write_manifest(folder: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(folder.join(".omfp"))?;
    std::fs::write(folder.join(".omfp/views.json"), text)?;
    Ok(())
}

// spec: E2E-11, AGT-011, UI-101, UI-102
#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_manifest_shows_the_default_view_and_two_views_switch() -> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    let thread = journey.create_thread("views journey").await?;
    let folder = journey.workspace_folder(&thread).await?;

    // An invalid manifest: the default view and a banner with the daemon's error.
    write_manifest(
        &folder,
        r#"{ "views": { "a": { "layout": ["row", {}] } } }"#,
    )?;
    journey.element("[data-testid=view-error]").await?;
    let banner = journey.text("[data-testid=view-error]").await?;
    assert!(
        banner.contains("views.a.layout: row has no children"),
        "{banner}"
    );
    journey
        .element("[data-testid=view][data-view=default]")
        .await?;
    journey.screenshot("m4-error-banner").await?;

    // Fixed with two views: the banner goes, the default view of the manifest shows.
    let manifest = json!({
        "default": "a",
        "views": {
            "a": { "title": "Alpha", "layout": ["row", {}, ["col", {}, "omfp:header", "omfp:chat"], "omfp:workspace"] },
            "b": { "title": "Beta", "layout": ["col", {}, "omfp:header", "omfp:workspace"] }
        }
    });
    write_manifest(&folder, &serde_json::to_string_pretty(&manifest)?)?;
    journey.gone("[data-testid=view-error]").await?;
    journey.element("[data-testid=view][data-view=a]").await?;

    // The frame's switcher shows the other view, and `:view` brings the first one back.
    journey.click("[data-testid=view-switcher] button").await?;
    journey
        .click("[data-testid=select-option][data-key=b]")
        .await?;
    journey.element("[data-testid=view][data-view=b]").await?;
    assert_eq!(
        journey
            .count("[data-testid=view] [data-widget=\"omfp:chat\"]")
            .await?,
        0
    );
    journey.command("view a").await?;
    journey.element("[data-testid=view][data-view=a]").await?;
    journey.finish("E2E-11").await
}

/// The manifest omfp wrote to the workspace's `views.json`, once it holds a view whose layout satisfies `check`.
async fn written_layout(
    journey: &Journey,
    folder: &Path,
    what: &str,
    check: impl Fn(&Value) -> bool,
) -> anyhow::Result<Value> {
    let file = folder.join(".omfp/views.json");
    journey
        .until(what, async |_| {
            let Ok(text) = std::fs::read_to_string(&file) else {
                return Ok(None);
            };
            let manifest: Value = serde_json::from_str(&text)?;
            let layout = manifest["views"]["default"]["layout"].clone();
            Ok(check(&layout).then_some(layout))
        })
        .await
}

/// Opens the Move menu of the widget at `from` and moves it to `place` (`before`, `after`, `end`) of the node at `to`.
async fn move_widget(journey: &Journey, from: &str, place: &str, to: &str) -> anyhow::Result<()> {
    journey
        .click(&format!(
            "[data-testid=widget-toolbar][data-path=\"{from}\"] [data-testid=widget-move]"
        ))
        .await?;
    journey
        .click(&format!(
            "[data-testid=move-target][data-place={place}][data-path=\"{to}\"]"
        ))
        .await
}

fn references(layout: &Value) -> Vec<String> {
    match layout {
        Value::String(reference) => vec![reference.clone()],
        Value::Array(items) => match items.first() {
            Some(Value::String(head))
                if ["row", "col", "tabs", "stack"].contains(&head.as_str()) =>
            {
                items.iter().skip(2).flat_map(references).collect()
            }
            Some(Value::String(head)) => vec![head.clone()],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

// spec: E2E-12, UI-103, UI-104, UI-106, AGT-021, AGT-022
#[tokio::test(flavor = "multi_thread")]
async fn the_user_edits_the_layout_adds_a_widget_and_replaces_a_built_in_with_a_copy()
-> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    let thread = journey.create_thread("layout journey").await?;
    let folder = journey.workspace_folder(&thread).await?;
    assert!(!folder.join(".omfp/views.json").exists());

    // Edit mode: move the workspace widget (the default view's `0.1`) before the header (`0.0.0`); the first change
    // writes the default view with it.
    journey.click("[data-testid=layout-edit]").await?;
    journey
        .element("[data-testid=widget-toolbar][data-path=\"0.1\"]")
        .await?;
    move_widget(&journey, "0.1", "before", "0.0.0").await?;
    let layout = written_layout(&journey, &folder, "the moved workspace widget", |layout| {
        references(layout) == ["omfp:workspace", "omfp:header", "omfp:chat"]
    })
    .await?;
    assert_eq!(
        layout,
        json!(["row", { "sizes": [3] }, ["col", {}, "omfp:workspace", "omfp:header", "omfp:chat"]])
    );

    // Hide the header (now `0.0.1`).
    journey
        .element("[data-testid=widget-toolbar][data-path=\"0.0.1\"][data-widget=\"omfp:header\"]")
        .await?;
    journey
        .click("[data-testid=widget-toolbar][data-path=\"0.0.1\"] [data-testid=widget-hide]")
        .await?;
    written_layout(&journey, &folder, "the hidden header", |layout| {
        references(layout) == ["omfp:workspace", "omfp:chat"]
    })
    .await?;

    // The picker adds it back at the end of the root container.
    journey.click("[data-testid=widget-add]").await?;
    journey.element("[data-testid=widget-picker]").await?;
    journey
        .element("[data-testid=widget-option][data-widget=\"omfp:header\"]")
        .await?;
    journey.screenshot("m4-widget-picker").await?;
    journey
        .click("[data-testid=widget-option][data-widget=\"omfp:header\"]")
        .await?;
    written_layout(&journey, &folder, "the added header", |layout| {
        references(layout) == ["omfp:workspace", "omfp:chat", "omfp:header"]
    })
    .await?;
    journey.gone("[data-testid=widget-picker]").await?;
    journey.click("[data-testid=layout-edit]").await?;
    journey.gone("[data-testid=widget-toolbar]").await?;

    // A copy of the built-in workspace widget replaces it in this workspace; a change to the copy shows.
    journey.command("widget copy omfp:workspace").await?;
    let copy = folder.join(".omfp/widgets/workspace");
    journey
        .until("the copied widget folder", async |_| {
            Ok(copy.join("index.ts").is_file().then_some(()))
        })
        .await?;
    let slot = "[data-testid=widget][data-widget=\"omfp:workspace\"][data-source=workspace]";
    journey
        .element(&format!("{slot} [data-testid=rename-workspace]"))
        .await?;
    std::fs::write(
        copy.join("index.tsx"),
        "import Original from \"./index.ts\";\n\n\
         export default function Modified(props: Record<string, unknown>) {\n\
         \x20 return (\n\
         \x20   <div data-testid=\"modified-workspace\">\n\
         \x20     <Original {...props} />\n\
         \x20   </div>\n\
         \x20 );\n\
         }\n",
    )?;
    journey
        .element(&format!(
            "{slot} [data-testid=modified-workspace] [data-testid=rename-workspace]"
        ))
        .await?;
    journey.finish("E2E-12").await
}
