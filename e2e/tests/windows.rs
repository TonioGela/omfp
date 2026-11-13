//! The agent's verification loop (06-testing.md, E2E-28): through the protocol, as an agent's `omfp_call` would, the
//! journey lists the open window and reads the views its init file added, switches it to one of them with
//! `windows.run`, then checks a widget no view shows with `windows.check_widget`: the window renders it out of sight
//! and reports its render error, then, once the file is fixed, what it renders. Run with `just test-e2e`.

use std::path::Path;

use e2e::Journey;
use omfp_protocol::method::{ThreadGit, WindowsCheckWidget, WindowsList, WindowsRun, WindowsState};
use omfp_protocol::{
    CheckWidgetParams, ListWindowsParams, RunInWindowParams, ThreadRef, ViewSource,
    WidgetReportState, WindowRef,
};

/// The workspace's init file: a view of its own.
const INIT: &str = r#"export default function init(omfp) {
  omfp.views.transform((views) =>
    views.add("solo", { title: "Solo", layout: ["col", {}, "omfp:header", "omfp:chat"] }),
  );
}
"#;

fn write(folder: &Path, relative: &str, text: &str) -> anyhow::Result<()> {
    let path = folder.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)?;
    Ok(())
}

// spec: E2E-28, PRT-122, PRT-123, PRT-124, UI-350, UI-352, UI-353, DMN-206, DMN-207
#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::too_many_lines)] // One acceptance journey, step by step.
async fn an_agent_lists_the_window_switches_its_view_and_checks_a_widget_out_of_sight()
-> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    let thread = journey.create_thread("windows journey").await?;
    let folder = journey.workspace_folder(&thread).await?;
    let mut agent = journey.connect().await?;
    let workspace_id = agent
        .call::<ThreadGit>(ThreadRef {
            thread_id: thread.clone(),
        })
        .await?
        .workspace_id;
    write(&folder, ".omfp/init.ts", INIT)?;
    journey
        .element("[data-testid=mode-view][data-view=solo]")
        .await?;

    // The window, as the agent sees it: its thread, and the init file's view with where it comes from.
    let window = journey
        .until("the window's report of the init file's view", async |_| {
            let listed = agent
                .call::<WindowsList>(ListWindowsParams {
                    workspace_id: Some(workspace_id.clone()),
                })
                .await?;
            let Some(window) = listed.windows.first() else {
                return Ok(None);
            };
            let state = agent
                .call::<WindowsState>(WindowRef {
                    window_id: window.window_id,
                })
                .await?;
            Ok(state
                .views
                .iter()
                .any(|view| view.id == "solo" && view.source == ViewSource::WorkspaceInit)
                .then_some(state))
        })
        .await?;
    assert_eq!(window.thread_id.as_deref(), Some(thread.as_str()));
    assert_eq!(window.active_view.as_deref(), Some("default"));

    // The agent shows the view in the window, as `:view solo` would.
    let ran = agent
        .call::<WindowsRun>(RunInWindowParams {
            window_id: None,
            workspace_id: Some(workspace_id.clone()),
            command: "view.switch".to_owned(),
            args: vec!["solo".to_owned()],
        })
        .await?;
    assert_eq!(ran.window_id, window.window_id);
    journey
        .element("[data-testid=view][data-view=solo]")
        .await?;
    let refused = agent
        .call::<WindowsRun>(RunInWindowParams {
            window_id: Some(window.window_id),
            workspace_id: None,
            command: "eval".to_owned(),
            args: vec!["alert(1)".to_owned()],
        })
        .await
        .unwrap_err()
        .to_string();
    assert!(refused.contains("no command \"eval\""), "{refused}");

    // A widget no view shows: the window renders it out of sight and reports its error.
    write(
        &folder,
        ".omfp/widgets/probe/index.tsx",
        "export default function Probe() {\n  throw new Error(\"probe exploded\");\n}\n",
    )?;
    let check = || CheckWidgetParams {
        window_id: None,
        workspace_id: workspace_id.clone(),
        widget: "probe".to_owned(),
    };
    let broken = agent.call::<WindowsCheckWidget>(check()).await?;
    assert!(broken.settled, "{broken:?}");
    assert_eq!(
        broken.report.state,
        WidgetReportState::RenderError,
        "{broken:?}"
    );
    assert!(
        broken
            .report
            .errors
            .iter()
            .any(|error| error.contains("probe exploded")),
        "{broken:?}"
    );
    assert_eq!(
        journey.count("[data-testid=offscreen-check]").await?,
        0,
        "unmounted"
    );
    assert_eq!(
        journey
            .count("[data-testid=view] [data-widget=\"./widgets/probe\"]")
            .await?,
        0,
        "never in a view"
    );

    // Fixed: the next check renders it.
    write(
        &folder,
        ".omfp/widgets/probe/index.tsx",
        "export default function Probe() {\n  return <p>probe ok</p>;\n}\n",
    )?;
    let fixed = agent.call::<WindowsCheckWidget>(check()).await?;
    assert!(fixed.settled, "{fixed:?}");
    assert_eq!(fixed.report.state, WidgetReportState::Rendered, "{fixed:?}");
    assert_eq!(fixed.report.text, "probe ok");
    assert_eq!(fixed.report.errors, Vec::<String>::new());
    agent.close().await?;
    journey.finish("E2E-28").await
}
