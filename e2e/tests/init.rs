//! The init file journey (06-testing.md, E2E-22, experimental, branch idea/init): a workspace's `.omfp/init.ts` adds a
//! command, a key binding and a view to the window, a syntax error in it shows a banner while the window keeps working,
//! and `:describe-key` says where the binding comes from. Run with `just test-e2e`.

use std::path::Path;

use e2e::Journey;
use thirtyfour::prelude::*;

/// The init file: `g h` switches to a view it adds.
const INIT: &str = r#"export default function init(omfp) {
  omfp.view("init-solo", { title: "Init solo", layout: ["col", {}, "omfp:header", "omfp:chat"] });
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
