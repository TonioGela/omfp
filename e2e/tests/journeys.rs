//! The critical journeys (06-testing.md, "End-to-end scenarios") against the TypeScript UI, each in a fresh browser,
//! daemon, omp environment and fake LLM. Run with `just test-e2e`.

use e2e::{Journey, row, system_prompt, workspace_roots};
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

/// The header's workspace and repository chips as `(kind, text)`.
async fn header_chips(journey: &Journey) -> anyhow::Result<Vec<(String, String)>> {
    let css = "[data-testid=thread-header] [data-testid=workspace-chip]";
    let kinds = journey.attrs(css, "data-kind").await?;
    let texts = journey.texts(css).await?;
    Ok(kinds
        .into_iter()
        .map(Option::unwrap_or_default)
        .zip(texts)
        .collect())
}

// spec: E2E-01, UI-001, UI-002, UI-012, UI-023, TST-031
#[tokio::test(flavor = "multi_thread")]
async fn a_thread_on_a_new_worktree_streams_its_reply_and_goes_idle() -> anyhow::Result<()> {
    let repo = repository("kumquat-17");
    let journey = Journey::start(vec![
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_bash_1",
            "bash",
            json!({ "command": "pwd && cat */AGENTS.md" }),
        )])),
        Step::reply(Reply::text("Hello from the new worktree.")),
    ])
    .await?;
    journey.connected().await?;
    // The token left the address bar (UI-002).
    let url = journey.driver.current_url().await?;
    assert_eq!(url.fragment(), None, "{url}");

    // A new workspace, a new worktree of a repository given by its path and the first message (UI-023).
    let before = journey.thread_ids().await?;
    journey.open_new_thread().await?;
    journey
        .type_into("[data-testid=new-workspace-name]", "worktree journey")
        .await?;
    journey.click("[data-testid=add-worktree]").await?;
    journey
        .element("[data-testid=repo-choice][data-kind=worktree]")
        .await?;
    journey
        .type_into(
            "[data-testid=worktree-repo]",
            &repo.path().to_string_lossy(),
        )
        .await?;
    journey
        .type_into("[data-testid=worktree-branch]", "feat-e2e")
        .await?;
    journey
        .type_into("[data-testid=new-thread-prompt]", "hello")
        .await?;
    journey.click("[data-testid=create-thread]").await?;
    let thread = journey.created(&before).await?;

    // The workspace chip, then the new worktree on its branch.
    let chips = journey
        .until("the worktree chip", async |page| {
            let chips = header_chips(page).await?;
            Ok((chips.len() == 2).then_some(chips))
        })
        .await?;
    assert_eq!(chips[0].0, "workspace", "{chips:?}");
    assert!(chips[0].1.contains("worktree journey"), "{chips:?}");
    assert_eq!(chips[1].0, "worktree", "{chips:?}");
    assert!(chips[1].1.contains("feat-e2e"), "{chips:?}");

    journey.reply("Hello from the new worktree.").await?;
    journey.status(&thread, "idle").await?;
    // The streamed reply is rendered as Markdown, and the first message is the thread's placeholder title.
    assert_eq!(
        journey
            .count("[data-testid=message][data-role=assistant] [data-testid=markdown]")
            .await?,
        1
    );
    assert_eq!(
        journey.attr(&row(&thread), "data-title-source").await?,
        Some("placeholder".to_owned())
    );
    // The agent runs in the workspace folder, the worktree inside it.
    let request = journey.fake.requests()[1].to_string();
    assert!(
        request.contains("/workspaces/worktree-journey"),
        "the cwd is the workspace folder: {request}"
    );
    assert!(
        request.contains("kumquat-17"),
        "the worktree is an entry of the workspace folder"
    );
    journey.finish("E2E-01").await
}

/// Clicks the option of the pending (unanswered) request whose text (omp's option label) contains `needle`.
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
    journey
        .attr(
            "[data-testid=interaction]:not([aria-busy=true])",
            "aria-label",
        )
        .await
}

/// Waits until the transcript shows exactly one tool card, in `status`.
async fn one_tool_card(journey: &Journey, status: &str) -> anyhow::Result<()> {
    journey
        .until(&format!("the tool card to be {status}"), async |page| {
            let statuses = page.attrs("[data-testid=tool-card]", "data-status").await?;
            Ok((statuses == vec![Some(status.to_owned())]).then_some(()))
        })
        .await
}

// spec: E2E-02, UI-070, UI-071, UI-050, TST-031
#[tokio::test(flavor = "multi_thread")]
async fn a_tool_needing_approval_completes_after_approving_it_in_the_ui() -> anyhow::Result<()> {
    let journey = Journey::start(vec![
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_bash_1",
            "bash",
            json!({ "command": "echo approved-run" }),
        )])),
        Step::reply(Reply::text("Done after approval.")),
    ])
    .await?;
    journey.connected().await?;
    let thread = journey.create_asking_thread("approval journey").await?;
    journey.send("run it").await?;

    journey.status(&thread, "awaiting").await?;
    journey
        .until("the approval request", async |page| {
            pending_request(page).await
        })
        .await?;
    // The first option of the first pending request has the focus (UI-070).
    journey.until_focused("interaction-option").await?;
    choose(&journey, "Approve").await?;

    one_tool_card(&journey, "succeeded").await?;
    journey.reply("Done after approval.").await?;
    journey.status(&thread, "idle").await?;
    assert!(
        journey.fake.requests()[1]
            .to_string()
            .contains("approved-run")
    );
    journey.finish("E2E-02").await
}

const ASK_FORM: &str = r#"<form>
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

// spec: E2E-03, UI-071
#[tokio::test(flavor = "multi_thread")]
async fn an_ask_with_two_questions_is_answered_and_the_agent_gets_the_answers() -> anyhow::Result<()>
{
    let journey = Journey::start(vec![
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_ask_1",
            "ask",
            json!({ "html": ASK_FORM }),
        )])),
        Step::reply(Reply::text("Using SQLite with Auth and Search.")),
    ])
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
            .gone("[data-testid=interaction][aria-busy=true]")
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

/// How many user and assistant messages and tool cards the transcript shows.
async fn transcript_counts(journey: &Journey) -> anyhow::Result<(usize, usize, usize)> {
    Ok((
        journey
            .count("[data-testid=message][data-role=user]")
            .await?,
        journey
            .count("[data-testid=message][data-role=assistant]")
            .await?,
        journey.count("[data-testid=tool-card]").await?,
    ))
}

// spec: E2E-04, UI-002, UI-003, UI-010
#[tokio::test(flavor = "multi_thread")]
async fn reloading_mid_turn_continues_the_transcript_without_gaps_or_duplicates()
-> anyhow::Result<()> {
    let journey = Journey::start(vec![
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_bash_1",
            "bash",
            json!({ "command": "echo before-reload" }),
        )])),
        Step::reply(Reply::text("Finished after the reload.")).gated(),
    ])
    .await?;
    journey.connected().await?;
    let thread = journey.create_thread("reload journey").await?;
    journey.send("go").await?;
    journey.fake.wait_for_gate(1).await;
    one_tool_card(&journey, "succeeded").await?;
    let before = transcript_counts(&journey).await?;

    // The token comes from `localStorage` now: the address bar lost it on the first load (UI-002).
    journey.driver.refresh().await?;
    journey.connected().await?;
    journey.click(&row(&thread)).await?;
    journey.opened(&thread).await?;
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

/// Waits until the queue shows a `kind` chip containing `text`.
async fn queued(journey: &Journey, kind: &str, text: &str) -> anyhow::Result<()> {
    journey
        .until(&format!("the {kind} chip {text:?}"), async |page| {
            let chips = queue(page).await?;
            Ok(chips
                .iter()
                .any(|(queue, chip)| queue == kind && chip.contains(text))
                .then_some(()))
        })
        .await
}

/// Waits until the send button is in `mode`.
async fn send_mode(journey: &Journey, mode: &str) -> anyhow::Result<()> {
    journey
        .until(&format!("the send button in {mode} mode"), async |page| {
            let modes = page.attrs("[data-testid=send]", "data-mode").await?;
            Ok((modes == vec![Some(mode.to_owned())]).then_some(()))
        })
        .await
}

// spec: E2E-05, UI-061, UI-062, UI-063
#[tokio::test(flavor = "multi_thread")]
async fn a_removed_follow_up_and_an_abort_bring_the_steer_back_to_the_composer()
-> anyhow::Result<()> {
    let journey = Journey::start(vec![Step::reply(Reply::text("Never sent.")).gated()]).await?;
    journey.connected().await?;
    let thread = journey.create_thread("queue journey").await?;
    journey.send("start").await?;
    journey.fake.wait_for_gate(0).await;
    journey.status(&thread, "running").await?;
    journey.busy().await?;

    // Enter while busy queues a follow-up (UI-061), which can be removed (UI-062).
    journey.send("later please").await?;
    queued(&journey, "follow-up", "later please").await?;
    journey.click("[data-testid=queue-remove]").await?;
    journey.gone("[data-testid=queue-chip]").await?;

    // Holding Alt in the composer switches the send button to steering; leaving the composer while it is held
    // does not leave it stuck there.
    let input = journey.element("[data-testid=composer-input]").await?;
    input.click().await?;
    journey
        .driver
        .action_chain()
        .key_down(Key::Alt)
        .perform()
        .await?;
    send_mode(&journey, "steer").await?;
    journey.click("[data-testid=transcript]").await?;
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
    queued(&journey, "steering", "steer me").await?;
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

/// Registers the repository at `path` from the Workspace panel; returns its id.
async fn register(journey: &Journey, path: &str) -> anyhow::Result<String> {
    let before = journey
        .attrs("[data-testid=repository-row]", "data-repository-id")
        .await?;
    journey
        .type_into("[data-testid=add-repository-path]", path)
        .await?;
    journey.click("[data-testid=add-repository-submit]").await?;
    journey
        .until("the registered repository", async |page| {
            let ids = page
                .attrs("[data-testid=repository-row]", "data-repository-id")
                .await?;
            Ok(ids.into_iter().find(|id| !before.contains(id)).flatten())
        })
        .await
}

/// Links the folder at `path` into the selected thread's workspace from the Workspace panel; returns the entry name.
async fn link(journey: &Journey, path: &str) -> anyhow::Result<String> {
    let css = "[data-testid=workspace-repo][data-kind=link]";
    let before = journey.attrs(css, "data-name").await?;
    journey
        .type_into("[data-testid=link-repo-path]", path)
        .await?;
    journey.click("[data-testid=link-repo-submit]").await?;
    journey
        .until("the linked repository", async |page| {
            let names = page.attrs(css, "data-name").await?;
            Ok(names
                .into_iter()
                .find(|name| !before.contains(name))
                .flatten())
        })
        .await
}

/// Adds a new worktree of `front` (registered first) and a link to `back` to the selected thread's workspace from the
/// Workspace panel.
async fn worktree_and_link(
    journey: &Journey,
    front: &TempRepo,
    back: &TempRepo,
) -> anyhow::Result<()> {
    let id = register(journey, &front.path().to_string_lossy()).await?;
    journey.select("worktree-repository", &id).await?;
    journey
        .type_into("[data-testid=worktree-branch-name]", "front-branch")
        .await?;
    journey
        .click("[data-testid=create-worktree-submit]")
        .await?;
    journey
        .element("[data-testid=workspace-repo][data-kind=worktree]")
        .await?;
    link(journey, &back.path().to_string_lossy()).await?;
    journey
        .until("the workspace and both repository chips", async |page| {
            Ok((header_chips(page).await?.len() == 3).then_some(()))
        })
        .await
}

/// Whether the focus is inside the element matching `css`.
async fn focus_inside(journey: &Journey, css: &str) -> anyhow::Result<bool> {
    let value = journey
        .eval(
            "const root = document.querySelector(arguments[0]);
             return !!root && root.contains(document.activeElement) && root !== document.activeElement;",
            vec![json!(css)],
        )
        .await?;
    Ok(value == json!(true))
}

// spec: E2E-08, UI-021, UI-022, UI-025, UI-092
#[tokio::test(flavor = "multi_thread")]
async fn a_thread_on_two_repositories_shows_under_both_when_grouped_by_repository()
-> anyhow::Result<()> {
    let front = repository("front-1");
    let back = repository("back-2");
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;

    // Space n (normal mode) opens the new-thread dialog with the focus in it (UI-092); no repository.
    let before = journey.thread_ids().await?;
    journey.keys(" n").await?;
    journey.element("[data-testid=new-thread-dialog]").await?;
    journey
        .until("the focus in the dialog", async |page| {
            Ok(focus_inside(page, "[data-testid=new-thread-dialog]")
                .await?
                .then_some(()))
        })
        .await?;
    journey
        .type_into("[data-testid=new-workspace-name]", "two repositories")
        .await?;
    journey.click("[data-testid=create-thread]").await?;
    let thread = journey.created(&before).await?;

    // A worktree and a link from two places, into the thread's workspace.
    worktree_and_link(&journey, &front, &back).await?;
    // The Workspace widget and its forms fit its column at a 1400 px window.
    journey.fits("[data-widget=\"omfp:workspace\"]").await?;

    journey.select("grouping", "repository").await?;
    let thread_row = row(&thread);
    let groups = journey
        .until("the thread under both repositories", async |page| {
            let groups = page
                .eval(
                    "return [...document.querySelectorAll('[data-testid=thread-group]')]
                       .filter(g => g.querySelector(arguments[0]))
                       .map(g => g.dataset.group);",
                    vec![json!(thread_row)],
                )
                .await?;
            let groups: Vec<String> = serde_json::from_value(groups)?;
            Ok((groups.len() == 2).then_some(groups))
        })
        .await?;
    assert_ne!(groups[0], groups[1]);
    journey.finish("E2E-08").await
}

/// The selector of the Workspace panel's entry `name`.
fn entry(name: &str) -> String {
    format!("[data-testid=workspace-repo][data-name=\"{name}\"]")
}

// spec: E2E-09, UI-025, DMN-029
#[tokio::test(flavor = "multi_thread")]
async fn workspace_folder_entries_follow_the_ui_and_the_agent_live() -> anyhow::Result<()> {
    let linked = repository("apricot-31");
    let source = repository("quince-58");
    let target = repository("medlar-94");
    let command = format!(
        "git -C {} worktree add -b agent-branch \"$PWD/agent-wt\" && ln -s {} agent-link",
        source.path().display(),
        target.path().display(),
    );
    let journey = Journey::start(vec![
        Step::reply(Reply::text("Linked and ready.")),
        Step::reply(Reply::tool_calls(vec![ToolCall::new(
            "call_bash_1",
            "bash",
            json!({ "command": command }),
        )])),
        Step::reply(Reply::text("Made a worktree and a link.")),
        Step::reply(Reply::text("One repository less.")),
    ])
    .await?;
    journey.connected().await?;
    let thread = journey.create_thread("folders journey").await?;

    // A link made in the Workspace panel shows there, and the next turn loads its AGENTS.md.
    let name = link(&journey, &linked.path().to_string_lossy()).await?;
    journey.send("hello").await?;
    journey.reply("Linked and ready.").await?;
    let system = system_prompt(&journey.fake.requests()[0]);
    assert!(
        system.contains("apricot-31"),
        "the link's AGENTS.md (roots {:?}): {system}",
        workspace_roots(&system)
    );
    let roots = workspace_roots(&system);
    let link_root = roots
        .iter()
        .find(|root| root.ends_with(&format!("/{name}")))
        .cloned()
        .unwrap_or_else(|| panic!("the link is a workspace root: {roots:?}"));

    // Entries the agent makes in the folder show up live.
    journey.send("add two more").await?;
    journey.reply("Made a worktree and a link.").await?;
    journey.element(&entry("agent-wt")).await?;
    journey.element(&entry("agent-link")).await?;
    assert_eq!(
        journey.attr(&entry("agent-wt"), "data-kind").await?,
        Some("worktree".to_owned())
    );
    assert_eq!(
        journey.attr(&entry("agent-link"), "data-kind").await?,
        Some("link".to_owned())
    );
    journey.status(&thread, "idle").await?;

    // Removing the link (no confirmation for a link) detaches it from the session with `/remove-dir`.
    journey
        .click(&format!("{} [data-testid=remove-repo]", entry(&name)))
        .await?;
    journey.gone(&entry(&name)).await?;
    journey.send("and now?").await?;
    journey.reply("One repository less.").await?;
    let system = system_prompt(&journey.fake.requests()[3]);
    let roots = workspace_roots(&system);
    assert!(
        !roots.contains(&link_root),
        "/remove-dir detached it: {roots:?}"
    );
    assert!(!system.contains("apricot-31"), "{system}");
    assert!(
        roots.iter().any(|root| root.ends_with("/agent-wt"))
            && roots.iter().any(|root| root.ends_with("/agent-link")),
        "the agent's entries were added: {roots:?}"
    );
    assert!(
        linked.path().join("AGENTS.md").is_file(),
        "the target stays"
    );
    journey.finish("E2E-09").await
}

/// Takes the focus out of every text field: the keymap's normal mode.
async fn normal_mode(journey: &Journey) -> anyhow::Result<()> {
    journey
        .eval(
            "if (document.activeElement) { document.activeElement.blur(); }",
            Vec::new(),
        )
        .await?;
    Ok(())
}

/// Presses `key` in normal mode and waits until thread `id` is selected.
async fn select_with(journey: &Journey, key: &str, id: &str) -> anyhow::Result<()> {
    journey.keys(key).await?;
    journey
        .until(&format!("{key} to select {id}"), async |page| {
            Ok((page.selected().await?.as_deref() == Some(id)).then_some(()))
        })
        .await
}

// spec: UI-095, UI-092
#[tokio::test(flavor = "multi_thread")]
async fn the_vim_keymap_moves_focuses_and_runs_the_command_line() -> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    journey.create_thread("first keys").await?;
    let second = journey.create_thread("second keys").await?;

    // j/k move the selection through the list in its order.
    normal_mode(&journey).await?;
    let order = journey.thread_ids().await?;
    assert_eq!(order.len(), 2, "{order:?}");
    let other = order.iter().find(|id| **id != second).unwrap().clone();
    let (forward, back) = if order[0] == second {
        ("j", "k")
    } else {
        ("k", "j")
    };
    select_with(&journey, forward, &other).await?;
    journey.opened(&other).await?;
    select_with(&journey, back, &second).await?;
    journey.opened(&second).await?;

    // i enters the composer; Esc leaves it (the thread is idle, so nothing is aborted).
    normal_mode(&journey).await?;
    journey.keys("i").await?;
    journey.until_focused("composer-input").await?;
    journey.keys(Key::Escape).await?;
    journey
        .until("the focus out of the composer", async |page| {
            Ok((page.focused().await?.as_deref() != Some("composer-input")).then_some(()))
        })
        .await?;

    // `:` opens the command line; `:theme dark` switches the colour scheme and closes it.
    normal_mode(&journey).await?;
    journey.keys(":").await?;
    journey.element("[data-testid=command-line]").await?;
    journey.until_focused("command-line-input").await?;
    journey.keys("theme dark").await?;
    journey.keys(Key::Enter).await?;
    journey.until_scheme("dark").await?;
    journey.gone("[data-testid=command-line]").await?;
    journey.finish("keymap").await
}

// spec: UI-091
#[tokio::test(flavor = "multi_thread")]
async fn a_theme_set_from_the_command_line_reaches_the_open_page_live() -> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    let built_in = journey.token("--omfp-primary").await?;
    assert!(!built_in.is_empty(), "the built-in theme sets the token");
    // A marker on `window` tells a reload apart from a live update.
    journey
        .eval("window.omfpE2eMarker = 'kept';", Vec::new())
        .await?;

    let dir = tempfile::tempdir()?;
    let file = dir.path().join("theme.css");
    std::fs::write(&file, ":root { --omfp-primary: #0b6e4f; }\n")?;
    let message = omfp::theme::run(omfp::theme::Command::Set(omfp::theme::SetArgs {
        file,
        daemon: omfp::remote::DaemonArgs {
            daemon: Some(format!("http://{}", journey.daemon().addr())),
            token: Some(journey.daemon().token().expose().to_owned()),
            data_dir: None,
        },
    }))
    .await?;
    assert!(message.starts_with("theme set from"), "{message}");

    journey
        .until("the pushed primary colour", async |page| {
            Ok((page.token("--omfp-primary").await? == "#0b6e4f").then_some(()))
        })
        .await?;
    let marker = journey
        .eval("return window.omfpE2eMarker || null;", Vec::new())
        .await?;
    assert_eq!(marker, json!("kept"), "the page did not reload");

    // Clearing it brings the built-in theme back, live too.
    let mut connection = journey.connect().await?;
    connection
        .call::<omfp_protocol::method::ThemeSet>(omfp_protocol::Theme { css: None })
        .await?;
    connection.close().await?;
    journey
        .until("the built-in primary colour", async |page| {
            Ok((page.token("--omfp-primary").await? == built_in).then_some(()))
        })
        .await?;
    journey.finish("live theme").await
}

/// Waits until `<html>` carries `data-density="<density>"`.
async fn until_density(journey: &Journey, density: &str) -> anyhow::Result<()> {
    journey
        .until(&format!("the {density} density"), async |page| {
            let value = page.attr("html", "data-density").await?;
            Ok((value.as_deref() == Some(density)).then_some(()))
        })
        .await
}

// spec: DMN-009, UI-097
#[tokio::test(flavor = "multi_thread")]
async fn editing_the_config_file_changes_the_open_page_live() -> anyhow::Result<()> {
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    until_density(&journey, "compact").await?;
    let compact = journey.token("--omfp-row-height").await?;
    journey
        .eval("window.omfpE2eMarker = 'kept';", Vec::new())
        .await?;

    // The user edits the daemon's config file while the page is open.
    let config = journey.config_file();
    std::fs::write(&config, "# edited by hand\n[ui]\ndensity = \"normal\"\n")?;
    until_density(&journey, "normal").await?;
    let normal = journey.token("--omfp-row-height").await?;
    assert_ne!(normal, compact, "the density changes the row height");
    let marker = journey
        .eval("return window.omfpE2eMarker || null;", Vec::new())
        .await?;
    assert_eq!(marker, json!("kept"), "the page did not reload");

    // And back.
    std::fs::write(&config, "[ui]\ndensity = \"compact\"\n")?;
    until_density(&journey, "compact").await?;
    assert_eq!(journey.token("--omfp-row-height").await?, compact);
    journey.finish("config file").await
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

/// Pastes [`DOT_PNG`] into the composer.
async fn paste_image(journey: &Journey) -> anyhow::Result<()> {
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
    Ok(())
}

fn long_text() -> String {
    (1..=200).fold(String::new(), |mut text, line| {
        text.push_str("line ");
        text.push_str(&line.to_string());
        text.push_str("\n\n");
        text
    })
}

// spec: UI-042, UI-060, UI-064
#[tokio::test(flavor = "multi_thread")]
async fn the_transcript_keeps_its_scroll_position_per_thread_and_pasted_images_attach()
-> anyhow::Result<()> {
    let journey = Journey::start(vec![
        Step::reply(Reply::text(long_text())),
        Step::reply(Reply::text("Short one.")),
    ])
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
    paste_image(&journey).await?;
    journey
        .until("the pasted image", async |page| {
            Ok((page.count("[data-testid=attachment]").await? == 1).then_some(()))
        })
        .await?;

    // Back on the long thread: where the user left it, unpinned, with its draft.
    journey.click(&row(&long_thread)).await?;
    journey.opened(&long_thread).await?;
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
    assert_eq!(draft(&journey).await?, "draft of the long thread");
    assert_eq!(journey.count("[data-testid=attachment]").await?, 0);
    journey.finish("browser behaviour").await
}

/// Waits until the sidebar shows the thread `thread_id` `count` times.
async fn until_rows(journey: &Journey, thread_id: &str, count: usize) -> anyhow::Result<()> {
    let css = row(thread_id);
    journey
        .until(&format!("{count} rows of {thread_id}"), async |page| {
            Ok((page.count(&css).await? == count).then_some(()))
        })
        .await
}

// spec: UI-026, UI-092
#[tokio::test(flavor = "multi_thread")]
async fn an_archived_thread_is_found_under_archived_unarchived_then_deleted() -> anyhow::Result<()>
{
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    let survivor = journey.create_thread("survivor").await?;
    let doomed = journey.create_thread("doomed").await?;

    // Archiving hides it and leaves no thread selected.
    journey.click("[data-testid=archive-thread]").await?;
    until_rows(&journey, &doomed, 0).await?;
    journey.element("[data-testid=no-thread]").await?;

    // The Archived filter lists only it; it opens from there and comes back with Unarchive.
    journey.select("filter-key", "archived").await?;
    until_rows(&journey, &doomed, 1).await?;
    assert_eq!(journey.count(&row(&survivor)).await?, 0);
    journey.click(&row(&doomed)).await?;
    journey.opened(&doomed).await?;
    journey.click("[data-testid=archive-thread]").await?;
    until_rows(&journey, &doomed, 0).await?;
    journey.select("filter-key", "all").await?;
    until_rows(&journey, &doomed, 1).await?;

    // Delete asks first, in a dialog that takes the focus; workspaces are kept.
    journey.click(&row(&doomed)).await?;
    journey.opened(&doomed).await?;
    journey.click("[data-testid=delete-thread]").await?;
    journey
        .element("[data-testid=delete-thread-dialog]")
        .await?;
    journey.until_focused("cancel-delete-thread").await?;
    journey.click("[data-testid=confirm-delete-thread]").await?;
    until_rows(&journey, &doomed, 0).await?;
    journey.gone("[data-testid=delete-thread-dialog]").await?;
    journey
        .until("the thread directory to go", async |page| {
            Ok((!page.data_dir().join("threads").join(&doomed).exists()).then_some(()))
        })
        .await?;

    // Gone after a reload too, under every filter.
    journey.driver.refresh().await?;
    journey.connected().await?;
    until_rows(&journey, &survivor, 1).await?;
    assert_eq!(journey.count(&row(&doomed)).await?, 0);
    journey.select("filter-key", "archived").await?;
    until_rows(&journey, &survivor, 0).await?;
    assert_eq!(journey.count(&row(&doomed)).await?, 0);
    journey.finish("archive and delete").await
}

// spec: UI-025
#[tokio::test(flavor = "multi_thread")]
async fn a_registered_repository_is_listed_starts_a_worktree_and_is_removed() -> anyhow::Result<()>
{
    let repo = repository("listed-3");
    let journey = Journey::start(Vec::new()).await?;
    journey.connected().await?;
    journey.create_thread("repositories").await?;
    journey.element("[data-testid=no-repositories]").await?;

    let id = register(&journey, &repo.path().to_string_lossy()).await?;
    let listed = format!("[data-testid=repository-row][data-repository-id=\"{id}\"]");
    let text = journey.text(&listed).await?;
    assert!(text.contains(&*repo.path().to_string_lossy()), "{text}");
    journey.gone("[data-testid=no-repositories]").await?;

    // Its "add to workspace" preselects it in the worktree form, which then makes a worktree in the workspace.
    journey
        .click(&format!("{listed} [data-testid=repository-new-worktree]"))
        .await?;
    journey
        .type_into("[data-testid=worktree-branch-name]", "listed-branch")
        .await?;
    journey
        .click("[data-testid=create-worktree-submit]")
        .await?;
    journey
        .element("[data-testid=workspace-repo][data-kind=worktree]")
        .await?;

    // Removing asks first and keeps the worktree.
    journey
        .click(&format!("{listed} [data-testid=remove-repository]"))
        .await?;
    journey
        .element("[data-testid=remove-repository-note]")
        .await?;
    journey
        .click("[data-testid=confirm-remove-repository]")
        .await?;
    journey.gone(&listed).await?;
    journey.element("[data-testid=no-repositories]").await?;
    assert_eq!(
        journey
            .count("[data-testid=workspace-repo][data-kind=worktree]")
            .await?,
        1
    );
    journey.finish("repositories").await
}
