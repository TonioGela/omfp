//! The critical journeys of M2 (06-testing.md, "End-to-end scenarios"), each in a fresh browser, daemon, omp
//! environment and fake LLM. Run with `just test-e2e`.

use e2e::{Journey, Settings};
use fake_llm::{Reply, Step, ToolCall};
use omfp_testkit::TempRepo;
use serde_json::json;
use thirtyfour::prelude::*;

fn repository(marker: &str) -> TempRepo {
    let repo = TempRepo::new().unwrap();
    repo.commit_file(
        "AGENTS.md",
        &format!("Marker: {marker}\n"),
        "add agents file",
    )
    .unwrap();
    repo
}

// spec: E2E-01, UI-001, UI-002, UI-012, UI-023
#[tokio::test(flavor = "multi_thread")]
async fn a_thread_on_a_new_worktree_streams_its_reply_and_goes_idle() -> anyhow::Result<()> {
    let repo = repository("kumquat-17");
    let journey = Journey::start(
        vec![Step::reply(Reply::text("Hello from the new worktree."))],
        Settings::default(),
    )
    .await?;
    journey.connected().await?;
    // The token left the address bar (UI-002).
    let url = journey.driver.current_url().await?;
    assert_eq!(url.fragment(), None, "{url}");

    journey.click("[data-testid=new-thread]").await?;
    journey
        .type_into("[data-testid=new-thread-title]", "worktree journey")
        .await?;
    journey.click("[data-testid=add-worktree]").await?;
    journey
        .type_into(
            "[data-testid=worktree-repo]",
            &repo.path().to_string_lossy(),
        )
        .await?;
    journey
        .type_into("[data-testid=worktree-branch]", "feat-e2e")
        .await?;
    journey.click("[data-testid=create-thread]").await?;
    let thread = journey.opened("worktree journey").await?;
    let chip = journey
        .text("[data-testid=thread-header] [data-testid=workspace-chip]")
        .await?;
    assert!(chip.contains("feat-e2e"), "{chip}");

    journey.send("hello").await?;
    journey.reply("Hello from the new worktree.").await?;
    journey.status(&thread, "idle").await?;
    let request = journey.fake.requests()[0].to_string();
    assert!(
        request.contains("kumquat-17"),
        "the worktree's AGENTS.md reaches the model"
    );
    journey.finish("E2E-01").await
}

/// Clicks the first option of the pending (unanswered) request whose text contains `needle`.
async fn choose(journey: &Journey, needle: &str) -> anyhow::Result<()> {
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
    assert_eq!(clicked, json!(true), "no option containing {needle:?}");
    Ok(())
}

/// The heading of the pending request, once one shows.
async fn pending_request(journey: &Journey) -> anyhow::Result<Option<String>> {
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

// spec: E2E-02, UI-070, UI-071, UI-050
#[tokio::test(flavor = "multi_thread")]
async fn a_tool_needing_approval_completes_after_approving_it_in_the_ui() -> anyhow::Result<()> {
    let journey = Journey::start(
        vec![
            Step::reply(Reply::tool_calls(vec![ToolCall::new(
                "call_bash_1",
                "bash",
                json!({ "command": "echo approved-run" }),
            )])),
            Step::reply(Reply::text("Done after approval.")),
        ],
        Settings {
            approval_mode: Some("always-ask"),
        },
    )
    .await?;
    journey.connected().await?;
    let thread = journey.create_thread("approval journey").await?;
    journey.send("run it").await?;

    journey.status(&thread, "awaiting").await?;
    journey
        .until("the approval request", async |page| {
            pending_request(page).await
        })
        .await?;
    // The first option of the first pending request has the focus (UI-070).
    let focused = journey
        .eval(
            "return document.activeElement && document.activeElement.dataset.testid;",
            Vec::new(),
        )
        .await?;
    assert_eq!(focused, json!("interaction-option"));
    choose(&journey, "Approve").await?;

    journey
        .until("the tool card to succeed", async |page| {
            let statuses = page.attrs("[data-testid=tool-card]", "data-status").await?;
            Ok((statuses == vec![Some("succeeded".to_owned())]).then_some(()))
        })
        .await?;
    journey.reply("Done after approval.").await?;
    journey.status(&thread, "idle").await?;
    assert!(
        journey.fake.requests()[1]
            .to_string()
            .contains("approved-run")
    );
    journey.finish("E2E-02").await
}

// spec: E2E-03, UI-071
#[tokio::test(flavor = "multi_thread")]
async fn an_ask_with_two_questions_is_answered_and_the_agent_gets_the_answers() -> anyhow::Result<()>
{
    let form = r#"<form>
  <fieldset><legend>Which database?</legend>
    <label><input type="radio" name="db" value="postgres"> Postgres</label>
    <label><input type="radio" name="db" value="sqlite" checked> SQLite</label>
  </fieldset>
  <fieldset><legend>Which features?</legend>
    <label><input type="checkbox" name="features" value="auth"> Auth</label>
    <label><input type="checkbox" name="features" value="billing"> Billing</label>
    <label><input type="checkbox" name="features" value="search"> Search</label>
  </fieldset>
</form>"#;
    let journey = Journey::start(
        vec![
            Step::reply(Reply::tool_calls(vec![ToolCall::new(
                "call_ask_1",
                "ask",
                json!({ "html": form }),
            )])),
            Step::reply(Reply::text("Using SQLite with Auth and Search.")),
        ],
        Settings::default(),
    )
    .await?;
    journey.connected().await?;
    let thread = journey.create_thread("ask journey").await?;
    journey.send("set it up").await?;

    // omp walks the form with one `select` dialog per question; the multiple choice repeats until "Done".
    let mut headings = Vec::new();
    let mut features = ["Auth", "Search", "Done"].into_iter();
    loop {
        let next = journey
            .until("a question or the answer", async |page| {
                if let Some(heading) = pending_request(page).await? {
                    return Ok(Some(Some(heading)));
                }
                let replies = page
                    .texts("[data-testid=message][data-role=assistant]")
                    .await?;
                Ok(replies
                    .iter()
                    .any(|reply| reply.contains("Using SQLite"))
                    .then_some(None))
            })
            .await?;
        let Some(heading) = next else { break };
        let choice = if heading.contains("database") {
            "SQLite"
        } else {
            features.next().expect("at most three feature questions")
        };
        choose(&journey, choice).await?;
        headings.push(heading);
        // The answered card closes before the next request shows.
        journey
            .until("the answered request to close", async |page| {
                let busy = page
                    .texts("[data-testid=interaction][aria-busy=true]")
                    .await?;
                Ok(busy.is_empty().then_some(()))
            })
            .await?;
    }
    assert!(
        headings
            .first()
            .is_some_and(|heading| heading.contains("Which database?")),
        "{headings:?}"
    );
    journey.status(&thread, "idle").await?;
    let second = journey.fake.requests()[1].to_string().to_lowercase();
    assert!(
        second.contains("sqlite") && second.contains("auth") && second.contains("search"),
        "the answers reach the model: {second}"
    );
    journey.finish("E2E-03").await
}

/// How many messages of `role` and tool cards the transcript shows.
async fn transcript_counts(journey: &Journey) -> anyhow::Result<(usize, usize, usize)> {
    let count = async |css: &str| -> anyhow::Result<usize> { Ok(journey.texts(css).await?.len()) };
    Ok((
        count("[data-testid=message][data-role=user]").await?,
        count("[data-testid=message][data-role=assistant]").await?,
        count("[data-testid=tool-card]").await?,
    ))
}

// spec: E2E-04, UI-002, UI-003, UI-010
#[tokio::test(flavor = "multi_thread")]
async fn reloading_mid_turn_continues_the_transcript_without_gaps_or_duplicates()
-> anyhow::Result<()> {
    let journey = Journey::start(
        vec![
            Step::reply(Reply::tool_calls(vec![ToolCall::new(
                "call_bash_1",
                "bash",
                json!({ "command": "echo before-reload" }),
            )])),
            Step::reply(Reply::text("Finished after the reload.")).gated(),
        ],
        Settings::default(),
    )
    .await?;
    journey.connected().await?;
    let thread = journey.create_thread("reload journey").await?;
    journey.send("go").await?;
    journey.fake.wait_for_gate(1).await;
    journey
        .until("the tool card before the reload", async |page| {
            let statuses = page.attrs("[data-testid=tool-card]", "data-status").await?;
            Ok((statuses == vec![Some("succeeded".to_owned())]).then_some(()))
        })
        .await?;
    let before = transcript_counts(&journey).await?;

    // The token comes from `localStorage` now: the address bar lost it on the first load (UI-002).
    journey.driver.refresh().await?;
    journey.connected().await?;
    journey
        .click(&format!(
            "[data-testid=thread-row][data-thread-id=\"{thread}\"]"
        ))
        .await?;
    journey.opened("reload journey").await?;
    journey
        .until("the transcript after the reload", async |page| {
            Ok((transcript_counts(page).await? == before).then_some(()))
        })
        .await?;
    journey.status(&thread, "running").await?;

    journey.fake.release_gate(1);
    journey.reply("Finished after the reload.").await?;
    journey.status(&thread, "idle").await?;
    let (users, assistants, tools) = transcript_counts(&journey).await?;
    assert_eq!((users, tools), (before.0, before.2));
    assert_eq!(assistants, before.1 + 1, "the reply is shown once");
    let replies = journey
        .texts("[data-testid=message][data-role=assistant]")
        .await?;
    assert_eq!(
        replies
            .iter()
            .filter(|reply| reply.contains("Finished after the reload."))
            .count(),
        1
    );
    journey.finish("E2E-04").await
}

/// The composer's current text.
async fn draft(journey: &Journey) -> anyhow::Result<String> {
    let value = journey
        .eval(
            "return document.querySelector('[data-testid=composer-input]').value;",
            Vec::new(),
        )
        .await?;
    Ok(value.as_str().unwrap_or_default().to_owned())
}

/// The queued chips as `(queue, text)`.
async fn queue(journey: &Journey) -> anyhow::Result<Vec<(String, String)>> {
    let kinds = journey
        .attrs("[data-testid=queue-chip]", "data-queue")
        .await?;
    let texts = journey.texts("[data-testid=queue-chip]").await?;
    Ok(kinds
        .into_iter()
        .map(Option::unwrap_or_default)
        .zip(texts)
        .collect())
}

// spec: E2E-05, UI-061, UI-062, UI-063
#[tokio::test(flavor = "multi_thread")]
async fn a_removed_follow_up_and_an_abort_bring_the_steer_back_to_the_composer()
-> anyhow::Result<()> {
    let journey = Journey::start(
        vec![Step::reply(Reply::text("Never sent.")).gated()],
        Settings::default(),
    )
    .await?;
    journey.connected().await?;
    let thread = journey.create_thread("queue journey").await?;
    journey.send("start").await?;
    journey.fake.wait_for_gate(0).await;
    journey.status(&thread, "running").await?;
    journey
        .until("the busy composer", async |page| {
            let busy = page.attrs("[data-testid=composer]", "data-busy").await?;
            Ok((busy == vec![Some("true".to_owned())]).then_some(()))
        })
        .await?;

    // Enter while busy queues a follow-up (UI-061), which can be removed (UI-062).
    journey.send("later please").await?;
    journey
        .until("the follow-up chip", async |page| {
            let chips = queue(page).await?;
            Ok(chips
                .iter()
                .any(|(kind, text)| kind == "follow-up" && text.contains("later please"))
                .then_some(()))
        })
        .await?;
    journey.click("[data-testid=queue-remove]").await?;
    journey
        .until("the follow-up to leave the queue", async |page| {
            Ok(queue(page).await?.is_empty().then_some(()))
        })
        .await?;

    // Holding Alt in the composer switches the send button to steering; leaving the composer while it is held
    // does not leave it stuck there.
    let input = journey.element("[data-testid=composer-input]").await?;
    input.click().await?;
    let send_mode = async |page: &Journey, mode: &str| {
        page.until(&format!("the send button in {mode} mode"), async |page| {
            let modes = page.attrs("[data-testid=send]", "data-mode").await?;
            Ok((modes == vec![Some(mode.to_owned())]).then_some(()))
        })
        .await
    };
    journey
        .driver
        .action_chain()
        .key_down(Key::Alt)
        .perform()
        .await?;
    send_mode(&journey, "steer").await?;
    journey.click("[data-testid=sidebar]").await?;
    journey
        .driver
        .action_chain()
        .key_up(Key::Alt)
        .perform()
        .await?;
    send_mode(&journey, "follow-up").await?;

    // Alt+Enter steers.
    input.click().await?;
    input.send_keys("steer me").await?;
    journey
        .driver
        .action_chain()
        .key_down(Key::Alt)
        .send_keys(Key::Enter)
        .key_up(Key::Alt)
        .perform()
        .await?;
    journey
        .until("the steering chip", async |page| {
            let chips = queue(page).await?;
            Ok(chips
                .iter()
                .any(|(kind, text)| kind == "steering" && text.contains("steer me"))
                .then_some(()))
        })
        .await?;
    assert_eq!(draft(&journey).await?, "");

    // Esc aborts; the queued steer comes back into the composer (UI-063).
    input.send_keys(Key::Escape).await?;
    journey
        .until("the restored draft", async |page| {
            Ok(draft(page).await?.contains("steer me").then_some(()))
        })
        .await?;
    journey.status(&thread, "idle").await?;
    assert_eq!(queue(&journey).await?, Vec::new());
    journey.fake.release_gate(0);
    journey.finish("E2E-05").await
}

/// Registers both repositories and creates one worktree of each from the Workspaces panel.
async fn two_worktrees(journey: &Journey, front: &TempRepo, back: &TempRepo) -> anyhow::Result<()> {
    // Two repositories, one worktree each, from the Workspaces panel.
    for repo in [&front, &back] {
        journey
            .type_into(
                "[data-testid=add-repository] input",
                &repo.path().to_string_lossy(),
            )
            .await?;
        journey.click("[data-testid=add-repository] button").await?;
    }
    journey
        .until("both repositories", async |page| {
            let options = page.texts("[data-testid=create-worktree] option").await?;
            Ok((options.len() == 3).then_some(()))
        })
        .await?;
    for (index, branch) in [(2, "front-branch"), (3, "back-branch")] {
        journey
            .element("[data-testid=create-worktree] select")
            .await?
            .find(By::Css(format!("option:nth-child({index})")))
            .await?
            .click()
            .await?;
        journey
            .type_into("[data-testid=create-worktree] input", branch)
            .await?;
        journey
            .click("[data-testid=create-worktree] button")
            .await?;
    }
    journey
        .until("both worktrees", async |page| {
            Ok((page.texts("[data-testid=workspace-row]").await?.len() == 2).then_some(()))
        })
        .await?;
    Ok(())
}

// spec: E2E-08, UI-021, UI-022, UI-025, UI-092
#[tokio::test(flavor = "multi_thread")]
async fn a_thread_on_two_repositories_shows_under_both_when_grouped_by_repository()
-> anyhow::Result<()> {
    let front = repository("front-1");
    let back = repository("back-2");
    let journey = Journey::start(Vec::new(), Settings::default()).await?;
    journey.connected().await?;

    two_worktrees(&journey, &front, &back).await?;

    // Alt+N opens the new-thread dialog with the title focused (UI-092); no workspace.
    journey
        .element("[data-testid=sidebar]")
        .await?
        .click()
        .await?;
    journey
        .driver
        .action_chain()
        .key_down(Key::Alt)
        .send_keys("n")
        .key_up(Key::Alt)
        .perform()
        .await?;
    journey.element("[data-testid=new-thread-dialog]").await?;
    let focused = journey
        .eval(
            "return document.activeElement && document.activeElement.dataset.testid;",
            Vec::new(),
        )
        .await?;
    assert_eq!(focused, json!("new-thread-title"));
    journey
        .type_into("[data-testid=new-thread-title]", "two repositories")
        .await?;
    journey.click("[data-testid=create-thread]").await?;
    let thread = journey.opened("two repositories").await?;

    // Attach both from the header, one after the other.
    for attached in 1..=2 {
        let attach = journey.element("[data-testid=attach-workspace]").await?;
        attach
            .find(By::Css("option:nth-child(2)"))
            .await?
            .click()
            .await?;
        journey
            .until(&format!("{attached} workspace chips"), async |page| {
                let chips = page
                    .texts("[data-testid=thread-header] [data-testid=workspace-chip]")
                    .await?;
                Ok((chips.len() == attached).then_some(()))
            })
            .await?;
    }

    journey
        .select("[data-testid=grouping]", "repository")
        .await?;
    let row = format!("[data-testid=thread-row][data-thread-id=\"{thread}\"]");
    let groups = journey
        .until("the thread under both repositories", async |page| {
            let groups = page
                .eval(
                    "return [...document.querySelectorAll('[data-testid=thread-group]')]
                       .filter(g => g.querySelector(arguments[0]))
                       .map(g => g.dataset.group);",
                    vec![json!(row)],
                )
                .await?;
            let groups: Vec<String> = serde_json::from_value(groups)?;
            Ok((groups.len() == 2).then_some(groups))
        })
        .await?;
    assert_ne!(groups[0], groups[1]);
    journey.finish("E2E-08").await
}

/// `scrollTop` and whether the transcript is scrolled to its end, with its thread and pinned flag.
async fn transcript_scroll(journey: &Journey) -> anyhow::Result<(f64, bool, String, String)> {
    let value = journey
        .eval(
            "const t = document.querySelector('[data-testid=transcript]');
             return [t.scrollTop, t.scrollTop + t.clientHeight >= t.scrollHeight - 2,
                     t.dataset.threadId || '', t.dataset.pinned || ''];",
            Vec::new(),
        )
        .await?;
    Ok(serde_json::from_value(value)?)
}

/// A 1×1 PNG.
const DOT_PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";

// spec: UI-042, UI-060, UI-064
#[tokio::test(flavor = "multi_thread")]
async fn the_transcript_keeps_its_scroll_position_per_thread_and_pasted_images_attach()
-> anyhow::Result<()> {
    let long: String = (1..=200).fold(String::new(), |mut text, line| {
        text.push_str("line ");
        text.push_str(&line.to_string());
        text.push_str("\n\n");
        text
    });
    let journey = Journey::start(
        vec![
            Step::reply(Reply::text(long)),
            Step::reply(Reply::text("Short one.")),
        ],
        Settings::default(),
    )
    .await?;
    journey.connected().await?;
    let long_thread = journey.create_thread("long thread").await?;
    journey.send("talk a lot").await?;
    journey.reply("line 200").await?;
    // Pinned to the end while the reply streams in.
    let (_, at_end, shown, pinned) = journey
        .until("the transcript at its end", async |page| {
            let scroll = transcript_scroll(page).await?;
            Ok(scroll.1.then_some(scroll))
        })
        .await?;
    assert_eq!(
        (at_end, shown.as_str(), pinned.as_str()),
        (true, long_thread.as_str(), "true")
    );

    // Scrolling up with the wheel unpins.
    journey
        .eval(
            "const t = document.querySelector('[data-testid=transcript]');
             t.dispatchEvent(new WheelEvent('wheel', { deltaY: -400, bubbles: true }));
             t.scrollTop = 400;",
            Vec::new(),
        )
        .await?;
    let (scrolled, ..) = journey
        .until("the transcript to unpin", async |page| {
            let scroll = transcript_scroll(page).await?;
            Ok((scroll.3 == "false").then_some(scroll))
        })
        .await?;

    // Another thread starts at its own end; its draft is its own (UI-064).
    journey
        .type_into("[data-testid=composer-input]", "draft of the long thread")
        .await?;
    let short_thread = journey.create_thread("short thread").await?;
    journey.send("briefly").await?;
    journey.reply("Short one.").await?;
    let (_, at_end, shown, pinned) = transcript_scroll(&journey).await?;
    assert_eq!(
        (at_end, shown.as_str(), pinned.as_str()),
        (true, short_thread.as_str(), "true")
    );

    // A pasted image becomes an attachment (UI-060).
    journey
        .eval(
            "const bytes = Uint8Array.from(atob(arguments[0]), c => c.charCodeAt(0));
             const data = new DataTransfer();
             data.items.add(new File([bytes], 'dot.png', { type: 'image/png' }));
             const input = document.querySelector('[data-testid=composer-input]');
             input.focus();
             input.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));",
            vec![json!(DOT_PNG)],
        )
        .await?;
    journey
        .until("the pasted image", async |page| {
            Ok((page.texts("[data-testid=attachment]").await?.len() == 1).then_some(()))
        })
        .await?;

    // Back on the long thread: where the user left it, unpinned, with its draft.
    journey
        .click(&format!(
            "[data-testid=thread-row][data-thread-id=\"{long_thread}\"]"
        ))
        .await?;
    journey.opened("long thread").await?;
    let (top, _, shown, pinned) = journey
        .until("the long thread's scroll position", async |page| {
            let scroll = transcript_scroll(page).await?;
            Ok((scroll.2 == long_thread && (scroll.0 - scrolled).abs() < 2.0).then_some(scroll))
        })
        .await?;
    assert_eq!(
        (shown.as_str(), pinned.as_str()),
        (long_thread.as_str(), "false"),
        "{top}"
    );
    let draft = journey
        .eval(
            "return document.querySelector('[data-testid=composer-input]').value;",
            Vec::new(),
        )
        .await?;
    assert_eq!(draft, json!("draft of the long thread"));
    assert_eq!(
        journey.texts("[data-testid=attachment]").await?,
        Vec::<String>::new()
    );
    journey.finish("browser behaviour").await
}
