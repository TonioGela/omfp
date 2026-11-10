/**
 * The clock port: the session waits through a {@link Timer} (`setTimeout` for real, a manual one in tests), so
 * reconnect backoff is testable without real time (TST-003).
 */

/** Runs callbacks after a delay. */
export interface Timer {
  /** Calls `fire` once, `delayMs` milliseconds from now; the returned function cancels it. */
  schedule(delayMs: number, fire: () => void): () => void;
}

/** {@link Timer} on the global `setTimeout`. */
export const systemTimer: Timer = {
  schedule(delayMs, fire) {
    const handle = setTimeout(fire, delayMs);
    return () => clearTimeout(handle);
  },
};
