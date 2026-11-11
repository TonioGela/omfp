/**
 * Live voice view model (UI-180..UI-186): what the live panel and the mic button show for one thread, from the
 * thread's reducer state (`live`, `live_error`, PRT-052), this window's own session ({@link LocalLive}) and this
 * window's connection number (PRT-051). Pure.
 */

import type { LiveLine, LivePhase, ThreadState } from "@omfp/core";
import type { LiveLineView, LiveStatus } from "@omfp/kit";

/** Whether this browser can hold a live session (UI-180): a secure context, WebRTC and a microphone API. */
export type LiveSupport = "available" | "insecure" | "unsupported";

/** This window's own session (at most one): the browser half of `live_connect`. */
export type LocalLive =
  | { readonly kind: "idle" }
  /** Microphone, peer and offer, then `thread.live_connect` until its answer is applied. */
  | { readonly kind: "starting"; readonly threadId: string }
  | {
      readonly kind: "running";
      readonly threadId: string;
      /** The peer reached `connected` (UI-181: omp's phase shows only from then on). */
      readonly connected: boolean;
      /** The remote voice is audible (UI-184). */
      readonly speaking: boolean;
      /** The local track is disabled (UI-183). */
      readonly muted: boolean;
    }
  /** The start failed before a session ran (microphone, peer, or the daemon's refusal), until dismissed. */
  | { readonly kind: "failed"; readonly threadId: string; readonly message: string };

export const IDLE: LocalLive = { kind: "idle" };

/** What the live panel shows. */
export type LivePanelView =
  | { readonly kind: "none" }
  /** This window's session: spectrum, keys. */
  | {
      readonly kind: "own";
      readonly status: LiveStatus;
      readonly lines: readonly LiveLineView[];
      readonly muted: boolean;
    }
  /** Another window's session (UI-182): its state and an end button, no audio. */
  | {
      readonly kind: "elsewhere";
      readonly status: LiveStatus;
      readonly lines: readonly LiveLineView[];
      readonly client: string;
    }
  /** A failed start or a session that ended on an error (UI-186). */
  | { readonly kind: "error"; readonly message: string };

/** The mic button of the composer (UI-180). */
export type MicButton =
  | { readonly kind: "start" }
  /** A session runs on the thread (this window's or another's): pressing ends it. */
  | { readonly kind: "end" }
  /** This browser cannot hold a session: pressing only says why. */
  | { readonly kind: "unavailable"; readonly reason: "insecure" | "unsupported" };

/** The latest line of each role, in the order they first appeared (UI-182). */
export function liveLines(transcript: readonly LiveLine[]): LiveLineView[] {
  const latest = new Map<LiveLine["role"], number>();
  transcript.forEach((line, index) => {
    latest.set(line.role, index);
  });
  return [...latest.values()]
    .sort((a, b) => a - b)
    .flatMap((index) => {
      const line = transcript[index];
      return line === undefined ? [] : [{ role: line.role, text: line.text }];
    });
}

/** omp's phase as a status; `connecting` until omp reports one. */
function phaseStatus(phase: LivePhase | null): LiveStatus {
  return phase ?? "connecting";
}

/**
 * The status of this window's session: `connecting` until the peer is connected (speech before that is lost, UI-181),
 * then omp's phase, `listening` turning into `speaking` while the remote voice is audible (UI-184).
 */
export function ownStatus(phase: LivePhase | null, connected: boolean, speaking: boolean): LiveStatus {
  if (!connected) {
    return "connecting";
  }
  const status = phaseStatus(phase);
  return status === "listening" && speaking ? "speaking" : status;
}

type LiveState = Pick<ThreadState, "live" | "live_error">;

/** The live panel of thread `threadId`; `connection` is this window's (`hello.connection`), `dismissed` the error the
 * user dismissed on this thread. */
export function livePanelView(
  threadId: string,
  state: LiveState,
  local: LocalLive,
  connection: number | undefined,
  dismissed: string | undefined,
): LivePanelView {
  const live = state.live;
  const mine = live !== null && live.owner.connection === connection;
  if (local.kind !== "idle" && local.threadId === threadId) {
    if (local.kind === "failed") {
      return { kind: "error", message: local.message };
    }
    const lines = mine ? liveLines(live.transcript) : [];
    if (local.kind === "starting") {
      return { kind: "own", status: "connecting", lines, muted: false };
    }
    const status = ownStatus(mine ? live.phase : null, local.connected, local.speaking);
    return { kind: "own", status, lines, muted: local.muted };
  }
  if (live !== null) {
    // This window's session it no longer holds is ending (its `live_end` is on the way).
    return mine
      ? { kind: "none" }
      : {
          kind: "elsewhere",
          status: phaseStatus(live.phase),
          lines: liveLines(live.transcript),
          client: live.owner.client,
        };
  }
  if (state.live_error !== null && state.live_error !== dismissed) {
    return { kind: "error", message: state.live_error };
  }
  return { kind: "none" };
}

/** The mic button of thread `threadId`. */
export function micButton(threadId: string, state: LiveState, local: LocalLive, support: LiveSupport): MicButton {
  const ownHere = (local.kind === "starting" || local.kind === "running") && local.threadId === threadId;
  if (ownHere || state.live !== null) {
    return { kind: "end" };
  }
  return support === "available" ? { kind: "start" } : { kind: "unavailable", reason: support };
}
