//! The which-key journey (06-testing.md, experimental, branch idea/init). E2E-26: `Space` opens the leader menu at
//! once, it follows the sequence and the key picked runs its command; a prefix shows its panel after the delay and
//! Escape cancels it; `Space Space` opens the command line. Run with `just test-e2e`.

use e2e::Journey;
use thirtyfour::prelude::*;

const PANEL: &str = "[data-testid=which-key]";

/// Leaves any text field, so the window is in normal mode.
async fn normal_mode(journey: &Journey) -> anyhow::Result<()> {
    journey
        .eval(
            "if (document.activeElement) { document.activeElement.blur(); }",
            Vec::new(),
        )
        .await?;
    Ok(())
}

// spec: E2E-26, UI-320, UI-321, UI-322, UI-324, UI-325, UI-326
#[tokio::test(flavor = "multi_thread")]
async fn space_opens_the_leader_menu_and_runs_the_binding_picked() -> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    journey.create_thread("which-key journey").await?;

    // `Space` shows the leader menu at once, in groups, its groups as prefixes.
    normal_mode(&journey).await?;
    journey.keys(" ").await?;
    journey
        .element("[data-testid=which-key][data-keys=Space]")
        .await?;
    let groups = journey
        .attrs("[data-testid=which-key-group]", "data-group")
        .await?;
    for group in ["threads", "views", "workspace", "files", "help", "commands"] {
        assert!(
            groups.iter().any(|found| found.as_deref() == Some(group)),
            "{group} missing: {groups:?}"
        );
    }
    let help = journey
        .text("[data-testid=which-key-entry][data-key=h]")
        .await?;
    assert!(help.contains("help…"), "{help}");
    journey.screenshot("which-key-leader").await?;

    // `h` follows the sequence; `k` picks `:describe-key`, which opens as the panel closes.
    journey.keys("h").await?;
    journey
        .element("[data-testid=which-key][data-keys='Space h']")
        .await?;
    journey
        .element("[data-testid=which-key-entry][data-command='help.describeKey']")
        .await?;
    journey.keys("k").await?;
    journey.gone(PANEL).await?;
    journey.element("[data-testid=describe-prompt]").await?;
    journey.keys(Key::Escape).await?;
    journey.gone("[data-testid=describe]").await?;

    // A prefix shows its panel after the delay; Escape drops it and runs nothing.
    normal_mode(&journey).await?;
    journey.keys("g").await?;
    journey
        .element("[data-testid=which-key][data-keys=g]")
        .await?;
    journey
        .element("[data-testid=which-key-entry][data-command='nav.top']")
        .await?;
    journey.keys(Key::Escape).await?;
    journey.gone(PANEL).await?;
    assert_eq!(journey.count("[data-testid=command-line]").await?, 0);

    // `Space Space` opens the command line.
    normal_mode(&journey).await?;
    journey.keys("  ").await?;
    journey.until_focused("command-line-input").await?;
    assert_eq!(journey.count(PANEL).await?, 0);
    journey.keys(Key::Escape).await?;
    journey.gone("[data-testid=command-line]").await?;
    Ok(())
}
