//! Readable live voice failures (DMN-104): omp's messages, as the daemon answers them and the reducer names a
//! session's end.

/// What omp says when the `openai-codex` login is missing (S-09).
const NO_CREDENTIAL: [&str; 2] = [
    "No Codex OAuth credential is available for a live call.",
    "No OAuth credential available for provider: openai-codex",
];

/// The readable form of omp's live failure `message`.
#[must_use]
pub fn live_error_message(message: &str) -> String {
    if NO_CREDENTIAL.iter().any(|known| message.contains(known)) {
        "live voice needs an `openai-codex` login in omp: log in with `/login openai-codex` in omp, then try again"
            .to_owned()
    } else if message.contains("Codex live signaling failed (401)")
        || message.contains("Codex live signaling failed (403)")
    {
        "live voice: the `openai-codex` login was rejected; log in again with `/login openai-codex` in omp"
            .to_owned()
    } else if message.contains("A live session is already active") {
        "a live session is already running on this thread".to_owned()
    } else if message.contains("End dictation before starting live voice") {
        "end dictation before starting live voice".to_owned()
    } else {
        format!("live voice failed: {message}")
    }
}
