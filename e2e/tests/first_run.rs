//! The first run (E2E-43), against a daemon on the fake launcher: when omp can use no model the window welcomes with
//! `Log in to a provider` instead of the view, and a login through the picker shows the view once omp can use models
//! (UI-500..UI-503, DMN-235, DMN-236); when omp has a model the window shows its normal frame (UI-504). A login another
//! connection started (an agent's) shows its page and progress in the window too (E2E-62).

use e2e::Journey;
use omfp_daemon::agent::{FakeLauncher, FakeReply};
use serde_json::json;

const WELCOME: &str = "[data-testid=welcome]";
const VIEW: &str = "[data-testid=view]";

/// Waits until `css` matches exactly `count` elements.
async fn until_count(journey: &Journey, css: &str, count: usize) -> anyhow::Result<()> {
    journey
        .until(&format!("{count} × {css}"), async |page| {
            Ok((page.count(css).await? == count).then_some(()))
        })
        .await
}

// spec: E2E-43, UI-500, UI-501, UI-502, UI-503, DMN-235, DMN-236
#[tokio::test(flavor = "multi_thread")]
async fn without_a_usable_model_the_window_welcomes_with_the_login_and_shows_the_view_after_it()
-> anyhow::Result<()> {
    let launcher = FakeLauncher::new();
    launcher.set_usable_models(0);
    launcher.reply(
        "get_login_providers",
        FakeReply::Ok(json!({ "providers": [
            { "id": "anthropic", "name": "Anthropic (Claude Pro/Max)", "available": true, "authenticated": false }
        ]})),
    );
    let journey = Journey::start_fake(launcher.clone()).await?;
    journey.element(WELCOME).await?;
    journey.gone("[data-testid=connection-banner]").await?;

    assert_eq!(
        journey.count(VIEW).await?,
        0,
        "the welcome stands for the view"
    );
    assert_eq!(
        journey.text("[data-testid=welcome-login]").await?,
        "Log in to a provider"
    );
    assert_eq!(
        journey.count("[data-testid=left-column]").await?,
        1,
        "the threads column stays"
    );
    assert_eq!(launcher.model_checks().len(), 1);
    journey.screenshot("first-run-welcome").await?;

    // The login through the picker; afterwards omp can use models.
    launcher.set_usable_models(3);
    journey.click("[data-testid=welcome-login]").await?;
    journey
        .click("[data-testid=login-picker] [data-testid=login-provider]")
        .await?;
    until_count(&journey, VIEW, 1).await?;
    journey.gone("[data-testid=login-picker]").await?;
    assert_eq!(journey.count(WELCOME).await?, 0);
    assert_eq!(
        journey.text("[data-testid=success]").await?,
        "Logged in to Anthropic (Claude Pro/Max)"
    );
    journey.screenshot("first-run-logged-in").await?;
    journey.finish("first run without a model").await
}

// spec: E2E-43, UI-504
#[tokio::test(flavor = "multi_thread")]
async fn with_a_usable_model_the_window_shows_its_normal_frame() -> anyhow::Result<()> {
    let launcher = FakeLauncher::new();
    let journey = Journey::start_fake(launcher.clone()).await?;
    journey.connected().await?;

    journey
        .until("the window's auth.status", async |_| {
            Ok((launcher.model_checks().len() == 1).then_some(()))
        })
        .await?;
    until_count(&journey, VIEW, 1).await?;
    assert_eq!(journey.count(WELCOME).await?, 0);
    journey.screenshot("first-run-with-a-model").await?;
    journey.finish("first run with a model").await
}

// spec: E2E-62, UI-640, DMN-290
#[tokio::test(flavor = "multi_thread")]
async fn a_login_another_connection_started_shows_its_page_and_progress_in_the_window()
-> anyhow::Result<()> {
    use omfp_core::wire::{ExtensionUiRequest, NotifyUiRequest, OpenUrlUiRequest, RpcNotification};
    const URL: &str = "https://auth.kimi.com/device?code=E2E62";
    let launcher = FakeLauncher::new();
    launcher.reply(
        "get_login_providers",
        FakeReply::Ok(json!({ "providers": [
            { "id": "kimi-code", "name": "Kimi Code", "available": true, "authenticated": false }
        ]})),
    );
    let gate = launcher.gate("login");
    let journey = Journey::start_fake(launcher.clone()).await?;
    journey.connected().await?;

    // An agent's `auth.login` (its own connection, as `omfp_call` has): its flow's requests go to that connection.
    let mut agent = journey.connect().await?;
    let login = tokio::spawn(async move {
        let logged_in = agent
            .call::<omfp_protocol::method::AuthLogin>(omfp_protocol::ProviderRef {
                provider_id: "kimi-code".to_owned(),
            })
            .await;
        agent.close().await?;
        logged_in
    });
    gate.arrived(1).await;
    let omp = launcher.process(launcher.launches().len() - 1);
    let ui = |request| RpcNotification::ExtensionUiRequest(request);
    omp.emit(vec![
        ui(ExtensionUiRequest::OpenUrl(OpenUrlUiRequest {
            id: "u1".into(),
            url: URL.into(),
            launch_url: None,
            instructions: Some("Enter E2E62".into()),
        })),
        ui(ExtensionUiRequest::Notify(NotifyUiRequest {
            id: "u2".into(),
            message: "Waiting for the browser…".into(),
            notify_type: None,
        })),
    ]);

    // The window shows it as it shows its own: the progress with Cancel, the page with Open and Copy.
    journey
        .until("the login's progress", async |page| {
            Ok(page
                .texts("[data-testid=login-progress-message]")
                .await?
                .contains(&"Waiting for the browser…".to_owned())
                .then_some(()))
        })
        .await?;
    assert!(
        journey
            .text("[data-testid=login-progress]")
            .await?
            .contains("Logging in to Kimi Code… finish in your browser")
    );
    let toast = format!("[data-testid=opened-url-toast][data-url='{URL}']");
    assert!(
        journey
            .text(&toast)
            .await?
            .contains("Log in to Kimi Code at auth.kimi.com")
    );
    assert_eq!(
        journey
            .text(&format!("{toast} [data-testid=login-instructions]"))
            .await?,
        "Enter E2E62"
    );
    assert_eq!(
        journey
            .count(&format!(
                "{toast} [data-testid=opened-url-open], {toast} [data-testid=opened-url-copy]"
            ))
            .await?,
        2
    );
    journey.screenshot("agent-login").await?;

    // It ends: its toasts go.
    gate.open();
    login.await??;
    journey.gone("[data-testid=login-progress]").await?;
    journey.gone("[data-testid=opened-url-toast]").await?;
    journey.finish("a login started elsewhere").await
}
