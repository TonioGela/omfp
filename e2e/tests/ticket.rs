//! Opening the UI with a one-time ticket (E2E-22, experimental, branch idea/ticket): the URL the tray opens and
//! `omfp daemon` prints carries `#ticket=` instead of the token (DMN-186); the page trades it for the token in
//! `system.hello`, keeps the token, and a second use of the same URL is refused with a word about it (UI-255).

use e2e::Journey;
use omfp_daemon::agent::FakeLauncher;
use omfp_daemon::ticket::DEFAULT_TTL;
use serde_json::json;

/// Opens `url` as a new page load: a navigation that only changes the fragment would not load the page again.
async fn open(journey: &Journey, url: &str) -> anyhow::Result<()> {
    journey.driver.goto("about:blank").await?;
    journey.driver.goto(url).await?;
    Ok(())
}

// spec: E2E-22, UI-255, DMN-186, PRT-096
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_url_connects_once_and_the_page_keeps_the_token() -> anyhow::Result<()> {
    let journey = Journey::start_fake(FakeLauncher::new()).await?;
    journey.connected().await?;
    // A browser that never saw the token.
    journey
        .eval("localStorage.clear(); return null;", Vec::new())
        .await?;
    let token = journey.daemon().token().expose().to_owned();
    let url = journey.daemon().ticket_url(DEFAULT_TTL)?;
    assert!(!url.contains(&token), "{url}");

    open(&journey, &url).await?;
    journey.connected().await?;
    // The ticket left the address bar; the page keeps the token it was traded for, not the ticket.
    let current = journey.driver.current_url().await?;
    assert_eq!(current.fragment(), None, "{current}");
    let stored = journey
        .eval("return localStorage.getItem('omfp.token');", Vec::new())
        .await?;
    assert_eq!(stored, json!(token));

    // A reload connects with the stored token.
    journey.driver.refresh().await?;
    journey.connected().await?;

    // The same URL again: the ticket is used up, and the page says so.
    open(&journey, &url).await?;
    journey.element("[data-testid=token-rejected]").await?;
    let said = journey.text("[data-testid=token-rejected]").await?;
    assert!(said.contains("already used"), "{said}");
    // The stored token still works once the address bar is clean.
    open(&journey, &journey.daemon().base_url()).await?;
    journey.connected().await?;
    journey.finish("E2E-22").await
}
