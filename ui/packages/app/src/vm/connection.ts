/** The connection banner (UI-003) and the status line announced through the ARIA live region (UI-092). */

import type { ConnectionStatus } from "@omfp/client";
import type { ThreadSummary } from "@omfp/protocol";
import { strings } from "../strings/index.ts";
import { displayTitle, isArchived, statusLabel, threadStatus, waitingCount } from "./threads.ts";

const text = strings.shell.connection;

export type Tone = "info" | "warning" | "error";

/** The non-blocking banner shown while not connected. */
export interface Banner {
  readonly tone: Tone;
  readonly text: string;
}

/** The banner for `connection`; none once connected. */
export function banner(connection: ConnectionStatus): Banner | undefined {
  switch (connection.kind) {
    case "connected":
      return undefined;
    case "connecting":
      return { tone: "info", text: text.connecting };
    case "reconnecting":
      return { tone: "warning", text: text.reconnecting(Math.ceil(connection.retryInMs / 1000), connection.attempt) };
    case "unauthenticated":
      return { tone: "error", text: text.unauthenticated };
    case "version_mismatch":
      return { tone: "error", text: text.versionMismatch(connection.message) };
    case "closed":
      return { tone: "error", text: text.closed };
  }
}

/**
 * The status line for screen readers: connection trouble, the selected thread's state, and how many threads wait for
 * input. Changes of this text are announced (UI-092).
 */
export function announcement(
  connection: ConnectionStatus,
  threads: readonly ThreadSummary[],
  selected: string | undefined,
): string {
  const parts: string[] = [];
  const shown = banner(connection);
  if (shown !== undefined) {
    parts.push(shown.text);
  }
  const thread = threads.find((candidate) => candidate.id === selected && !isArchived(candidate));
  if (thread !== undefined) {
    parts.push(text.threadState(displayTitle(thread), statusLabel(threadStatus(thread))));
  }
  const waiting = waitingCount(threads);
  if (waiting === 1) {
    parts.push(text.awaitingOne);
  } else if (waiting > 1) {
    parts.push(text.awaitingMany(waiting));
  }
  return parts.join(". ");
}
