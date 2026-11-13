/**
 * Readable live voice failures (port of `omfp-core/src/live.rs`, DMN-104): omp's messages, as the daemon answers them
 * and the reducer names a session's end.
 */

/** What omp says when the `openai-codex` login is missing (S-09). */
const NO_CREDENTIAL = [
  "No Codex OAuth credential is available for a live call.",
  "No OAuth credential available for provider: openai-codex",
];

/** The readable form of omp's live failure `message`. */
export function liveErrorMessage(message: string): string {
  if (NO_CREDENTIAL.some((known) => message.includes(known))) {
    return "live voice needs an `openai-codex` login in omp: run `:login openai-codex`, then try again";
  }
  if (message.includes("Codex live signaling failed (401)") || message.includes("Codex live signaling failed (403)")) {
    return "live voice: the `openai-codex` login was rejected; log in again with `:login openai-codex`";
  }
  if (message.includes("A live session is already active")) {
    return "a live session is already running on this thread";
  }
  if (message.includes("End dictation before starting live voice")) {
    return "end dictation before starting live voice";
  }
  return `live voice failed: ${message}`;
}
