//! Screenshots of the main screens for a visual check (06-testing.md, L5): a busy thread with a Markdown reply, a
//! tool card and a queued follow-up in both themes, a pending approval, an `ask` question and the new-thread dialog.
//! The screenshots land in `/tmp` (or `OMFP_SCREENSHOT_DIR`); nothing compares pixels. Run with `just test-e2e`.

use e2e::{Journey, Settings};
use fake_llm::{Reply, Step, ToolCall};
use serde_json::json;

const MARKDOWN: &str = "# Release plan

Here is the **plan**, starting with `cargo xtask spec-trace`:

- build the web bundle
- run the journeys with `just test-e2e`
  - nested: look at every screenshot

## Code

```rust
fn main() {
    println!(\"hello from the release plan\");
}
```

| Step | Status | Owner |
|------|--------|-------|
| build | done | ci |
| test | running | `nextest` |
";

/// Switches the theme and waits until the app shows it.
async fn theme(journey: &Journey, theme: &str) -> anyhow::Result<()> {
    journey.select("[data-testid=theme]", theme).await?;
    journey
        .until(&format!("the {theme} theme"), async |page| {
            let themes = page.attrs("[data-testid=app]", "data-theme").await?;
            Ok((themes == vec![Some(theme.to_owned())]).then_some(()))
        })
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_busy_thread_in_both_themes_and_the_new_thread_dialog() -> anyhow::Result<()> {
    let journey = Journey::start(
        vec![
            Step::reply(Reply::tool_calls(vec![ToolCall::new(
                "call_bash_1",
                "bash",
                json!({ "command": "echo screenshot-run && ls -la" }),
            )])),
            Step::reply(Reply::text(MARKDOWN)),
            Step::reply(Reply::text("Working on the next step.")).gated(),
            Step::reply(Reply::text("The follow-up is done.")),
        ],
        Settings::default(),
    )
    .await?;
    journey.connected().await?;
    let thread = journey.create_thread("screenshot journey").await?;
    journey.send("show me the plan").await?;
    journey.reply("hello from the release plan").await?;
    journey.status(&thread, "idle").await?;

    // A second prompt held by the model keeps the thread busy while a follow-up is queued.
    journey.send("next step").await?;
    journey.fake.wait_for_gate(2).await;
    journey.status(&thread, "running").await?;
    journey
        .until("the busy composer", async |page| {
            let busy = page.attrs("[data-testid=composer]", "data-busy").await?;
            Ok((busy == vec![Some("true".to_owned())]).then_some(()))
        })
        .await?;
    journey.send("and a follow-up after that").await?;
    journey
        .until("the queued follow-up", async |page| {
            let chips = page
                .texts("[data-testid=queue-chip]:not([aria-busy=true])")
                .await?;
            Ok((chips.len() == 1).then_some(()))
        })
        .await?;

    theme(&journey, "light").await?;
    journey.fits("[data-testid=side-panels]").await?;
    journey.screenshot("a-thread-light").await?;
    theme(&journey, "dark").await?;
    journey.screenshot("b-thread-dark").await?;

    journey.fake.release_gate(2);
    journey.reply("The follow-up is done.").await?;
    journey.status(&thread, "idle").await?;

    theme(&journey, "light").await?;
    journey.click("[data-testid=new-thread]").await?;
    journey
        .type_into("[data-testid=new-thread-title]", "a thread on a worktree")
        .await?;
    journey.click("[data-testid=add-worktree]").await?;
    journey
        .type_into(
            "[data-testid=worktree-repo]",
            "/home/someone/projects/a-repository-with-a-long-name",
        )
        .await?;
    journey
        .type_into("[data-testid=worktree-branch]", "feat-screenshots")
        .await?;
    journey.fits("[data-testid=new-thread-dialog]").await?;
    journey.screenshot("d-new-thread-dialog").await?;
    journey.finish("screenshots: thread and dialog").await
}

/// The heading of the pending (unanswered) request, once one shows.
async fn pending(journey: &Journey) -> anyhow::Result<Option<String>> {
    Ok(journey
        .attrs(
            "[data-testid=interaction]:not([aria-busy=true])",
            "aria-label",
        )
        .await?
        .into_iter()
        .next()
        .flatten())
}

/// Clicks the first option containing `needle` of the pending request, then waits until it closes.
async fn answer(journey: &Journey, needle: &str) -> anyhow::Result<()> {
    let clicked = journey
        .eval(
            "const option = [...document.querySelectorAll(
                 '[data-testid=interaction]:not([aria-busy=true]) [data-testid=interaction-option]')]
               .find(e => e.innerText.includes(arguments[0]));
             if (option) { option.click(); }
             return !!option;",
            vec![json!(needle)],
        )
        .await?;
    anyhow::ensure!(clicked == json!(true), "no option containing {needle:?}");
    journey
        .until("the answered request to close", async |page| {
            let busy = page
                .texts("[data-testid=interaction][aria-busy=true]")
                .await?;
            Ok(busy.is_empty().then_some(()))
        })
        .await
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pending_approval_and_an_ask_question() -> anyhow::Result<()> {
    let form = r#"<form>
  <fieldset><legend>Which database should the service use?</legend>
    <label><input type="radio" name="db" value="postgres"> Postgres</label>
    <label><input type="radio" name="db" value="sqlite" checked> SQLite</label>
  </fieldset>
</form>"#;
    let journey = Journey::start(
        vec![
            Step::reply(Reply::tool_calls(vec![ToolCall::new(
                "call_bash_1",
                "bash",
                json!({ "command": "rm -rf build && cargo build --release" }),
            )])),
            Step::reply(Reply::tool_calls(vec![ToolCall::new(
                "call_ask_1",
                "ask",
                json!({ "html": form }),
            )])),
            Step::reply(Reply::text("Using SQLite.")),
        ],
        Settings {
            approval_mode: Some("always-ask"),
        },
    )
    .await?;
    journey.connected().await?;
    theme(&journey, "light").await?;
    let thread = journey.create_thread("approval screenshot").await?;
    journey.send("build it").await?;

    // Approvals (bash, and the ask tool if it needs one) until the question shows; the first one is captured.
    let mut approvals = 0;
    loop {
        let heading = journey
            .until("a pending request", async |page| pending(page).await)
            .await?;
        if heading.contains("database") {
            break;
        }
        if approvals == 0 {
            journey.screenshot("c-approval").await?;
        }
        approvals += 1;
        answer(&journey, "Approve").await?;
    }
    anyhow::ensure!(approvals > 0, "no approval was asked");
    journey.screenshot("c-ask").await?;
    answer(&journey, "SQLite").await?;
    journey.reply("Using SQLite.").await?;
    journey.status(&thread, "idle").await?;
    journey.finish("screenshots: approval and ask").await
}
