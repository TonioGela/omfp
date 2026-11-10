/**
 * Queue chips (UI-062): rendered only from the last `queue_update` (`ThreadState.queue`). Remove and promote mark a
 * chip as pending; the chip itself changes only when the next `queue_update` arrives.
 */

import type { QueueState } from "@omfp/core";
import { isObject } from "./tool.ts";

export type QueueOp = "remove" | "promote";

export type QueueKind = "steering" | "follow-up";

/** A queue action sent and not yet answered. */
export interface PendingQueueOp {
  readonly threadId: string;
  readonly op: QueueOp;
  readonly message: string;
}

export interface QueueChip {
  readonly queue: QueueKind;
  readonly text: string;
  /** An action on this text waits for the daemon. */
  readonly pending: QueueOp | undefined;
  /** Follow-ups can be promoted to steering. */
  readonly canPromote: boolean;
}

/** Steering chips, then follow-ups, each in queue order. */
export function queueChips(queue: QueueState, threadId: string, pending: readonly PendingQueueOp[]): QueueChip[] {
  const chip = (kind: QueueKind, text: string): QueueChip => ({
    queue: kind,
    text,
    pending: pending.find((op) => op.threadId === threadId && op.message === text)?.op,
    canPromote: kind === "follow-up",
  });
  return [
    ...queue.steering.map((text) => chip("steering", text)),
    ...queue.follow_up.map((text) => chip("follow-up", text)),
  ];
}

/** The pending list once the daemon answered `op` (whatever it answered). */
export function settleQueueOp(pending: readonly PendingQueueOp[], done: PendingQueueOp): PendingQueueOp[] {
  return pending.filter((op) => !(op.threadId === done.threadId && op.op === done.op && op.message === done.message));
}

/** Whether the daemon applied a queue action: its `{ removed }` / `{ promoted }` answer (PRT, UI-062). */
export function queueOpApplied(op: QueueOp, result: unknown): boolean {
  return isObject(result) && result[op === "remove" ? "removed" : "promoted"] === true;
}
