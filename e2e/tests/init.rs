//! The init file journeys (06-testing.md, experimental, branch idea/init). E2E-22: a workspace's `.omfp/init.ts` adds
//! a command, a key binding and a view to the window, a syntax error in it shows a banner while the window keeps
//! working, and `:describe-key` says where the binding comes from. E2E-23: an init file adds a view computed from the
//! workspace's repositories and a TSX view, whose splitters resize for the window and whose syntax error shows the
//! default view until the next save. Run with `just test-e2e`.

use std::path::Path;

use e2e::Journey;
use thirtyfour::prelude::*;

/// The init file: `g h` switches to a view it adds.
const INIT: &str = r#"export default function init(omfp) {
  omfp.views.transform((views) =>
    views.add("init-solo", { title: "Init solo", layout: ["col", {}, "omfp:header", "omfp:chat"] }),
  );
  omfp.command({
    id: "demo.solo",
    title: "Show the init view",
    description: "Added by the workspace's init file.",
    run: () => {
      omfp.run("view.switch", ["init-solo"]);
    },
  });
  omfp.bind({ keys: "g h", command: "demo.solo" });
}
"#;

fn write_init(folder: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(folder.join(".omfp"))?;
    std::fs::write(folder.join(".omfp/init.ts"), text)?;
    Ok(())
}

/// Leaves any text field, so the window is in normal mode, then types `keys`.
async fn normal_keys(journey: &Journey, keys: &str) -> anyhow::Result<()> {
    journey
        .eval(
            "if (document.activeElement) { document.activeElement.blur(); }",
            Vec::new(),
        )
        .await?;
    journey.keys(keys).await
}

// spec: E2E-22, UI-260, UI-261, UI-262, UI-263, UI-264, DMN-185
#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_init_file_adds_a_bound_command_and_a_view_and_survives_a_syntax_error()
-> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    let thread = journey.create_thread("init journey").await?;
    let folder = journey.workspace_folder(&thread).await?;

    // The file arrives with the next poll; its view joins the switcher and its key runs its command.
    write_init(&folder, INIT)?;
    journey
        .until("the init file's binding", async |page| {
            normal_keys(page, "gh").await?;
            Ok((page
                .count("[data-testid=view][data-view=init-solo]")
                .await?
                == 1)
                .then_some(()))
        })
        .await?;
    journey.command("view default").await?;
    journey
        .element("[data-testid=view][data-view=default]")
        .await?;

    // `:describe-key` names the command, its description and the file that bound it.
    journey.command("describe-key").await?;
    journey.element("[data-testid=describe-prompt]").await?;
    journey.keys("gh").await?;
    journey
        .element("[data-testid=describe][data-state=key]")
        .await?;
    let described = journey.text("[data-testid=describe]").await?;
    for needle in [
        "demo.solo",
        "Show the init view",
        "Added by the workspace's init file.",
        "the workspace's .omfp/init.ts",
    ] {
        assert!(
            described.contains(needle),
            "{needle:?} missing: {described}"
        );
    }
    journey.screenshot("init-describe-key").await?;
    journey.keys(Key::Escape).await?;
    journey.gone("[data-testid=describe]").await?;

    // A syntax error: a banner names the file and the position; what the file added is gone, the window works.
    write_init(
        &folder,
        "export default function init(omfp) {\n  omfp.bind({ keys: \"g h\" \n}\n",
    )?;
    journey.element("[data-testid=init-error]").await?;
    let banner = journey.text("[data-testid=init-error]").await?;
    assert!(banner.contains(".omfp/init.ts"), "{banner}");
    assert!(banner.contains("init.ts:"), "{banner}");
    journey.screenshot("init-error").await?;

    // Fixed: the banner goes and the binding is back.
    write_init(&folder, INIT)?;
    journey.gone("[data-testid=init-error]").await?;
    journey
        .until("the init file's binding again", async |page| {
            normal_keys(page, "gh").await?;
            Ok((page
                .count("[data-testid=view][data-view=init-solo]")
                .await?
                == 1)
                .then_some(()))
        })
        .await?;
    journey.finish("E2E-22").await
}

/// The workspace's init file of E2E-23: `views.json`'s views, one computed from the repositories, one TSX file.
const VIEWS_INIT: &str = r#"export default function init(omfp) {
  const views = omfp.views.load("views.json");
  const repos = omfp.workspace.repos;
  views.add("review", {
    title: "Review",
    layout: ["row", {}, "omfp:chat", ...repos.map((r) => ["omfp:changes", { repo: r.name, title: r.name }])],
  });
  views.add("ops", { title: "Ops", view: "./views/ops.tsx" });
  omfp.views.use(views);
}
"#;

/// The TSX view of E2E-23.
const OPS: &str = r#"import { Col, Row, Tabs, Widget } from "omfp/sdk";

export default function Ops() {
  return (
    <Row sizes={[3, 2]}>
      <Tabs id="main"><Widget use="omfp:chat" /></Tabs>
      <Col sizes={[1, 1]}>
        <Widget use="omfp:changes" />
        <Tabs><Widget use="omfp:background" /><Widget use="omfp:mcp" /></Tabs>
      </Col>
    </Row>
  );
}
"#;

fn write_ops(folder: &Path, text: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(folder.join(".omfp/views"))?;
    std::fs::write(folder.join(".omfp/views/ops.tsx"), text)?;
    Ok(())
}

/// The share of the first cell the root row's splitter of the shown view gives (`aria-valuenow`).
async fn root_share(journey: &Journey) -> anyhow::Result<String> {
    let value = journey
        .eval(
            "const bar = document.querySelector('[data-testid=view] [data-testid=splitter][data-path=\"0\"]');\
             return bar === null ? null : bar.getAttribute('aria-valuenow');",
            Vec::new(),
        )
        .await?;
    Ok(value.as_str().unwrap_or_default().to_owned())
}

// spec: E2E-23, UI-267, UI-268, UI-300, UI-301, UI-302, DMN-186, AGT-074
#[tokio::test(flavor = "multi_thread")]
async fn an_init_file_adds_a_computed_view_and_a_tsx_view_that_resizes_and_survives_a_syntax_error()
-> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    let thread = journey.create_thread("init views journey").await?;
    let folder = journey.workspace_folder(&thread).await?;

    write_ops(&folder, OPS)?;
    write_init(&folder, VIEWS_INIT)?;
    // The computed view, a tab of the mode line: the chat beside the changes of each repository.
    journey
        .element("[data-testid=mode-view][data-view=review]")
        .await?;
    journey
        .click("[data-testid=mode-view][data-view=review]")
        .await?;
    journey
        .element("[data-testid=view][data-view=review]")
        .await?;
    journey
        .element("[data-testid=mode-view][data-view=review][aria-current=true]")
        .await?;
    // It exists only in the init file: `:widget add` names the file instead of writing views.json.
    journey.command("widget add omfp:todos").await?;
    let refused = journey.text("[data-testid=error]").await?;
    assert!(
        refused.contains("the workspace's .omfp/init.ts"),
        "{refused}"
    );

    // The TSX view, picked with `g v`: laid out like a manifest view, files open in its `main` tabs, its splitters
    // resize.
    journey
        .eval(
            "if (document.activeElement) { document.activeElement.blur(); }",
            Vec::new(),
        )
        .await?;
    journey.keys("gv").await?;
    journey.until_focused("command-line-input").await?;
    journey
        .click("[data-testid=command-option][data-value=\"view ops\"]")
        .await?;
    journey
        .element("[data-testid=view][data-view=ops] [data-slot=main]")
        .await?;
    assert_eq!(root_share(&journey).await?, "60");
    journey
        .eval(
            "document.querySelector('[data-testid=view] [data-testid=splitter][data-path=\"0\"]').focus();",
            Vec::new(),
        )
        .await?;
    journey.keys(Key::Right).await?;
    let resized = root_share(&journey).await?;
    assert_ne!(resized, "60", "the splitter moved");
    journey.screenshot("init-tsx-view").await?;

    // A syntax error: a banner names the file, the default view stands in, the window works.
    write_ops(
        &folder,
        "export default function Ops() {\n  return <Row>;\n}\n",
    )?;
    journey.element("[data-testid=view-file-error]").await?;
    let banner = journey.text("[data-testid=view-file-error]").await?;
    assert!(banner.contains(".omfp/views/ops.tsx"), "{banner}");
    assert!(banner.contains("ops.tsx:"), "{banner}");
    journey
        .element("[data-testid=view][data-view=default]")
        .await?;
    journey.screenshot("init-tsx-view-error").await?;

    // Fixed: the view is back with this window's sizes.
    write_ops(&folder, OPS)?;
    journey.gone("[data-testid=view-file-error]").await?;
    journey
        .element("[data-testid=view][data-view=ops] [data-slot=main]")
        .await?;
    assert_eq!(root_share(&journey).await?, resized);
    journey.finish("E2E-23").await
}
