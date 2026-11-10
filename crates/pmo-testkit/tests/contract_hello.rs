//! Contract test (L3): the pinned omp, in RPC mode, answers a prompt through the fake LLM.
//!
//! Run with `just test-contract` inside the dev shell.

use std::time::Duration;

use fake_llm::{FakeLlm, Reply, Script, Step};
use omp_rpc::{PromptCommand, PromptStatus};
use pmo_testkit::{
    DenyProxy, FAKE_MODEL, KNOWN_DISCOVERY_HOSTS, OmpEnv, OmpSession, require_pinned_omp,
};

const ANSWER: &str = "Hello from the fake model";

// spec: TST-012, TST-013, TST-021
#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_is_answered_by_the_fake_llm_without_other_network_access() {
    require_pinned_omp();
    let fake = FakeLlm::start(Script::new(
        FAKE_MODEL,
        vec![Step::reply(Reply::text(ANSWER))],
    ))
    .await
    .unwrap();
    let proxy = DenyProxy::start().await.unwrap();
    let env = OmpEnv::new(&fake.base_url(), Some(&proxy)).unwrap();

    let (turn, session_env) = tokio::task::spawn_blocking(move || {
        let session = OmpSession::start(&env, &["--no-session"]).unwrap();
        let prompt = PromptCommand {
            message: "say hi".into(),
            images: None,
            streaming_behavior: None,
        };
        let turn = session
            .client()
            .prompt_and_wait(&prompt, Duration::from_secs(60));
        let turn = turn.unwrap_or_else(|error| {
            panic!("prompt failed: {error}\nstderr:\n{}", session.stderr())
        });
        session
            .wait_for_frame("session_settled", Duration::from_secs(60))
            .unwrap();
        session.record_fixture(&env, "hello").unwrap();
        (turn, (session, env))
    })
    .await
    .unwrap();
    let (session, _env) = session_env;

    assert_eq!(
        turn.assistant_text.as_deref(),
        Some(ANSWER),
        "stderr:\n{}",
        session.stderr()
    );
    let result = turn.result.expect("the agent was invoked");
    assert_eq!(result.status, PromptStatus::Completed);

    assert!(
        fake.failures().is_empty(),
        "unexpected model requests: {:#?}",
        fake.failures()
    );
    assert!(fake.is_exhausted_cleanly(), "every scripted step was used");
    let requests = fake.requests();
    assert_eq!(requests[0]["model"], FAKE_MODEL);
    assert!(requests[0].to_string().contains("say hi"));

    let stray = proxy.unexpected_attempts(KNOWN_DISCOVERY_HOSTS);
    assert!(
        stray.is_empty(),
        "omp tried to reach hosts outside model discovery: {stray:#?}"
    );
}
