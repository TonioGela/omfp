//! Fork a thread (E2E-66), with the pinned omp and the fake LLM: *Fork from here* on a user message makes a new thread
//! holding the conversation up to that message, shown with where it comes from in its header and its row (UI-660,
//! UI-664, UI-665); the fork goes on by itself while the source keeps its conversation.

use e2e::{Journey, row};
use fake_llm::{Reply, Step};

const MESSAGES: &str = "[data-testid=message]";

/// Waits until the open transcript's messages are `texts` (each message's text contains its entry).
async fn transcript(journey: &Journey, texts: &[&str]) -> anyhow::Result<()> {
    journey
        .until(&format!("the transcript {texts:?}"), async |page| {
            let shown = page.texts(MESSAGES).await?;
            let matches = shown.len() == texts.len()
                && shown
                    .iter()
                    .zip(texts)
                    .all(|(shown, text)| shown.contains(text));
            Ok(matches.then_some(()))
        })
        .await
}

// spec: E2E-66, UI-660, UI-664, UI-665
#[tokio::test(flavor = "multi_thread")]
async fn fork_from_a_user_message_makes_a_new_thread_and_leaves_the_source_alone()
-> anyhow::Result<()> {
    let journey = Journey::start(vec![
        Step::reply(Reply::text("FIRST-ANSWER")),
        Step::reply(Reply::text("SECOND-ANSWER")),
        Step::reply(Reply::text("FORK-ANSWER")),
    ])
    .await?;
    journey.connected().await?;
    let source = journey.create_thread("fork journey").await?;
    journey.send("first question").await?;
    journey.reply("FIRST-ANSWER").await?;
    journey.status(&source, "idle").await?;
    journey.send("second question").await?;
    journey.reply("SECOND-ANSWER").await?;
    journey.status(&source, "idle").await?;
    let title = journey.text("[data-testid=thread-title]").await?;

    // The second question's Fork from here, shown while the message is hovered.
    let question = journey
        .driver
        .find_all(thirtyfour::By::Css(MESSAGES))
        .await?
        .into_iter()
        .nth(2)
        .ok_or_else(|| anyhow::anyhow!("no third message"))?;
    journey
        .driver
        .action_chain()
        .move_to_element_center(&question)
        .perform()
        .await?;
    journey.screenshot("fork-source").await?;
    let before = journey.thread_ids().await?;
    question
        .find(thirtyfour::By::Css("[data-testid=message-fork]"))
        .await?
        .click()
        .await?;
    let fork = journey.created(&before).await?;

    // The fork: its title, where it comes from, the conversation up to the question.
    journey
        .until("the fork's title", async |page| {
            let shown = page.text("[data-testid=thread-title]").await?;
            Ok((shown == format!("{title} (fork)")).then_some(()))
        })
        .await?;
    assert_eq!(
        journey.text("[data-testid=fork-lineage]").await?,
        format!("Forked from {title} at \u{201c}second question\u{201d}")
    );
    transcript(
        &journey,
        &["first question", "FIRST-ANSWER", "second question"],
    )
    .await?;
    assert_eq!(
        journey
            .count(&format!("{} [data-testid=thread-forked]", row(&fork)))
            .await?,
        1
    );
    assert_eq!(
        journey
            .count(&format!("{} [data-testid=thread-forked]", row(&source)))
            .await?,
        0
    );

    // It goes on by itself.
    journey.send("fork question").await?;
    journey.reply("FORK-ANSWER").await?;
    journey.status(&fork, "idle").await?;
    journey.screenshot("fork-forked").await?;

    // The lineage leads back to the source, which kept its conversation.
    journey.click("[data-testid=fork-lineage]").await?;
    journey.opened(&source).await?;
    transcript(
        &journey,
        &[
            "first question",
            "FIRST-ANSWER",
            "second question",
            "SECOND-ANSWER",
        ],
    )
    .await?;
    journey.finish("E2E-66").await
}
