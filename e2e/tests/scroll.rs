//! Transcript pinning while a tool runs (UI-042), in headless Chromium against a daemon on the fake launcher: a bash
//! call streams its output into an expanded card, which grows the transcript, while the user scrolls up with the
//! wheel (a real wheel event through CDP's `Input.dispatchMouseEvent`) and with `PageUp`. The view must stay where the
//! user scrolled to and unpinned; `End` pins it again and the output then keeps the bottom in view.

use e2e::{Journey, row};
use omfp_core::wire::{AgentMessage, RpcNotification};
use omfp_daemon::agent::{FakeLauncher, FakeProcess, FakeRun};
use omfp_protocol::method::ThreadsCreate;
use omfp_protocol::{CreateThreadParams, CreateWorkspaceParams, ThreadWorkspace};
use serde_json::{Value, json};
use thirtyfour::prelude::*;

const TRANSCRIPT: &str = "[data-testid=transcript]";
const CALL: &str = "call_running_bash";
const CARD: &str = "[data-testid=tool-card][data-call=call_running_bash]";

fn message(value: &Value) -> AgentMessage {
    serde_json::from_value(value.clone()).expect("a valid agent message")
}

fn frame(value: &Value) -> RpcNotification {
    serde_json::from_value(value.clone()).expect("a valid omp frame")
}

fn assistant(content: &Value) -> Value {
    json!({
        "role": "assistant", "api": "openai-completions", "provider": "fake", "model": "scripted",
        "content": content, "stopReason": "toolUse", "timestamp": 0,
        "usage": {
            "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
            "cost": { "input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0, "total": 0.0 },
        },
    })
}

/// Enough questions and answers to scroll.
fn history() -> Vec<AgentMessage> {
    (0..40)
        .flat_map(|index| {
            [
                json!({ "role": "user", "timestamp": 0, "content": [{ "type": "text", "text": format!("Question {index}?") }] }),
                assistant(&json!([{ "type": "text", "text": format!(
                    "Answer {index}.\n\nA paragraph long enough to take a few lines of the transcript, so that forty \
                     of them make a list much taller than the window."
                ) }])),
            ]
        })
        .map(|value| message(&value))
        .collect()
}

/// The turn so far: the assistant calls bash, and omp starts running it.
fn start_running() -> Vec<RpcNotification> {
    let call = assistant(&json!([{
        "type": "toolCall", "id": CALL, "name": "bash", "arguments": { "command": "make test" },
    }]));
    vec![
        frame(&json!({ "type": "agent_start" })),
        frame(&json!({ "type": "message_start", "messageId": "run", "message": call })),
        frame(&json!({ "type": "message_end", "messageId": "run", "message": call })),
        frame(&json!({
            "type": "tool_execution_start", "toolCallId": CALL, "toolName": "bash",
            "args": { "command": "make test" },
        })),
    ]
}

/// The output after `lines` lines, as omp streams it.
fn output(lines: usize) -> RpcNotification {
    let text: String = (1..=lines)
        .map(|line| format!("line {line} of the test run\n"))
        .collect::<Vec<_>>()
        .concat();
    frame(&json!({
        "type": "tool_execution_update", "toolCallId": CALL, "toolName": "bash",
        "args": { "command": "make test" },
        "partialResult": { "content": [{ "type": "text", "text": text }], "details": {} },
    }))
}

/// `(scrollTop, scrollHeight, pinned)` of the transcript.
async fn scroll_state(journey: &Journey) -> anyhow::Result<(f64, f64, bool)> {
    let value = journey
        .eval(
            "const t = document.querySelector(arguments[0]);
             return [t.scrollTop, t.scrollHeight, t.dataset.pinned === 'true'];",
            vec![json!(TRANSCRIPT)],
        )
        .await?;
    Ok(serde_json::from_value(value)?)
}

/// Streams `count` more output lines, one update each, waiting until the card shows each one.
async fn stream_lines(
    journey: &Journey,
    process: &FakeProcess,
    shown: &mut usize,
    count: usize,
) -> anyhow::Result<()> {
    for _ in 0..count {
        *shown += 1;
        process.emit(vec![output(*shown)]);
        let needle = format!("line {shown} of the test run");
        journey
            .until(&needle, async |page| {
                Ok(page.text(CARD).await?.contains(&needle).then_some(()))
            })
            .await?;
    }
    Ok(())
}

/// A real wheel turn over the transcript (`delta_y` < 0 scrolls up), through the compositor like a mouse.
async fn wheel(journey: &Journey, delta_y: f64) -> anyhow::Result<()> {
    let value = journey
        .eval(
            "const r = document.querySelector(arguments[0]).getBoundingClientRect();
             return [r.left + r.width / 2, r.top + r.height / 2];",
            vec![json!(TRANSCRIPT)],
        )
        .await?;
    let (x, y): (f64, f64) = serde_json::from_value(value)?;
    journey
        .driver
        .cdp()
        .send_raw(
            "Input.dispatchMouseEvent",
            json!({ "type": "mouseWheel", "x": x, "y": y, "deltaX": 0, "deltaY": delta_y }),
        )
        .await?;
    Ok(())
}

/// Waits until the transcript scrolled up from `bottom` and reports it unpinned; returns where it stopped.
async fn until_scrolled_up(journey: &Journey, bottom: f64, how: &str) -> anyhow::Result<f64> {
    journey
        .until(
            &format!("the transcript scrolled up with {how}"),
            async |page| {
                let (top, _, pinned) = scroll_state(page).await?;
                Ok((top < bottom - 100.0 && !pinned).then_some(top))
            },
        )
        .await
}

/// Streams ten more lines and checks the view stayed at `top`, unpinned.
async fn stays_at(
    journey: &Journey,
    process: &FakeProcess,
    shown: &mut usize,
    top: f64,
    how: &str,
) -> anyhow::Result<()> {
    stream_lines(journey, process, shown, 10).await?;
    let (after, _, pinned) = scroll_state(journey).await?;
    assert_eq!(
        (after, pinned),
        (top, false),
        "after scrolling up with {how}, the running tool's output must not pull the view back"
    );
    Ok(())
}

// spec: UI-042
#[tokio::test(flavor = "multi_thread")]
async fn scrolling_up_while_a_tool_streams_output_stays_where_the_user_scrolled()
-> anyhow::Result<()> {
    let launcher = FakeLauncher::new();
    launcher.push_run(FakeRun::fresh().with_history(history()));
    let journey = Journey::start_fake(launcher.clone()).await?;
    let mut connection = journey.connect().await?;
    let thread = connection
        .call::<ThreadsCreate>(CreateThreadParams {
            title: None,
            workspace: ThreadWorkspace::New(CreateWorkspaceParams {
                name: Some("scroll".to_owned()),
                link: None,
            }),
            model: None,
            thinking_level: None,
            approval_mode: None,
            prompt: None,
        })
        .await?
        .id;
    connection.close().await?;
    journey.click(&row(&thread)).await?;
    journey.opened(&thread).await?;

    // Opening the thread starts omp asynchronously: wait for the launch before driving it.
    journey
        .until("the thread's omp to start", async |_| {
            Ok((!launcher.launches().is_empty()).then_some(()))
        })
        .await?;
    // A running bash call whose card the user expanded: its output grows the transcript.
    let process = launcher.process(0);
    process.emit(start_running());
    journey
        .until("the running card", async |page| {
            Ok((page.count(CARD).await? == 1).then_some(()))
        })
        .await?;
    journey
        .eval(
            "document.querySelector(arguments[0] + ' [data-testid=toggle]').click();",
            vec![json!(CARD)],
        )
        .await?;
    let mut shown = 0;
    stream_lines(&journey, &process, &mut shown, 5).await?;
    let (bottom, _, pinned) = scroll_state(&journey).await?;
    assert!(pinned, "the view follows the bottom at first");

    // The wheel: the user scrolled up, the output keeps coming.
    wheel(&journey, -400.0).await?;
    let top = until_scrolled_up(&journey, bottom, "the wheel").await?;
    stays_at(&journey, &process, &mut shown, top, "the wheel").await?;

    // End pins the view again, and the output keeps the bottom in view.
    let bottom = back_to_bottom(&journey, &process, &mut shown).await?;

    // PageUp: the same.
    journey.keys(Key::PageUp).await?;
    let top = until_scrolled_up(&journey, bottom, "PageUp").await?;
    stays_at(&journey, &process, &mut shown, top, "PageUp").await?;

    // A scroll that comes with no wheel, key or pointer event — dragging the scrollbar in WebKitGTK, whose overlay
    // scrollbars send no pointer events to the page: the same.
    let bottom = back_to_bottom(&journey, &process, &mut shown).await?;
    journey
        .eval(
            "const t = document.querySelector(arguments[0]); t.scrollTop = t.scrollTop - 600;",
            vec![json!(TRANSCRIPT)],
        )
        .await?;
    let top = until_scrolled_up(&journey, bottom, "the scrollbar").await?;
    stays_at(&journey, &process, &mut shown, top, "the scrollbar").await?;

    journey.finish("scrolling while a tool runs").await
}

/// `End` on the focused transcript pins the view again; the output then keeps the bottom in view. Returns the
/// bottom's `scrollTop`.
async fn back_to_bottom(
    journey: &Journey,
    process: &FakeProcess,
    shown: &mut usize,
) -> anyhow::Result<f64> {
    journey
        .eval(
            "document.querySelector(arguments[0]).focus();",
            vec![json!(TRANSCRIPT)],
        )
        .await?;
    journey.keys(Key::End).await?;
    journey
        .until("the pinned transcript", async |page| {
            Ok(scroll_state(page).await?.2.then_some(()))
        })
        .await?;
    stream_lines(journey, process, shown, 5).await?;
    let (bottom, height, pinned) = scroll_state(journey).await?;
    let client: f64 = serde_json::from_value(
        journey
            .eval(
                "return document.querySelector(arguments[0]).clientHeight;",
                vec![json!(TRANSCRIPT)],
            )
            .await?,
    )?;
    assert!(
        pinned && height - bottom - client <= 32.0,
        "pinned at the bottom while the output grows"
    );
    Ok(bottom)
}
