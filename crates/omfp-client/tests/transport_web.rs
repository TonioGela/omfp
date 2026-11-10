//! The transport suite (TST-030, UI-001) for [`WebTransport`] in headless Chromium (`wasm-bindgen-test`), against the
//! daemon `just test-wasm` starts; its URL and token are baked in at compile time.

#![cfg(target_arch = "wasm32")]

mod suite;

use omfp_client::{GlooTimer, TransportError, WebTransport};
use suite::Harness;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

struct WebHarness;

impl Harness for WebHarness {
    type Transport = WebTransport;
    type Timer = GlooTimer;

    fn url(&self) -> String {
        env!(
            "OMFP_TEST_DAEMON_URL",
            "build through `just test-wasm`, which starts the daemon"
        )
        .to_owned()
    }

    fn token(&self) -> String {
        env!(
            "OMFP_TEST_DAEMON_TOKEN",
            "build through `just test-wasm`, which starts the daemon"
        )
        .to_owned()
    }

    fn timer(&self) -> GlooTimer {
        GlooTimer
    }

    fn open(url: String) -> impl Future<Output = Result<WebTransport, TransportError>> {
        WebTransport::connect(url)
    }
}

// spec: TST-030, UI-001, PRT-001, PRT-005
#[wasm_bindgen_test]
async fn hello() {
    suite::hello(&WebHarness).await;
}

// spec: TST-030, UI-001, PRT-002
#[wasm_bindgen_test]
async fn auth_failure() {
    suite::auth_failure(&WebHarness).await;
}

// spec: TST-030, UI-001, PRT-020, PRT-021
#[wasm_bindgen_test]
async fn subscribe_snapshot() {
    suite::subscribe_snapshot(&WebHarness).await;
}

// spec: TST-030, UI-001, UI-003, PRT-020
#[wasm_bindgen_test]
async fn replay() {
    suite::replay(&WebHarness).await;
}

// spec: TST-030, UI-001, UI-003
#[wasm_bindgen_test]
async fn reconnect() {
    suite::reconnect(&WebHarness).await;
}
