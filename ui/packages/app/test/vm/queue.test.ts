import { describe, expect, test } from "bun:test";
import { type PendingQueueOp, queueChips, queueOpApplied, settleQueueOp } from "../../src/vm/queue.ts";
import { fixture, fixturePrefix } from "../widgets/chat/fixtures.ts";

describe("queue chips", () => {
  // spec: UI-062
  test("chips follow the last queue_update and show pending actions", () => {
    const queued = fixturePrefix("follow_up_then_abort", 12);
    const pending: PendingQueueOp[] = [{ threadId: "t", op: "remove", message: "later please" }];
    expect(queueChips(queued.queue, "t", pending)).toEqual([
      { queue: "follow-up", text: "later please", pending: "remove", canPromote: true },
    ]);
    expect(queueChips(queued.queue, "other", pending)[0]?.pending).toBeUndefined();
    expect(queueChips(fixture("follow_up_then_abort").queue, "t", pending)).toEqual([]);
  });

  // spec: UI-062
  test("an answered action stops being pending; only removed/promoted true counts as applied", () => {
    const remove: PendingQueueOp = { threadId: "t", op: "remove", message: "m" };
    const promote: PendingQueueOp = { threadId: "t", op: "promote", message: "m" };
    expect(settleQueueOp([remove, promote], remove)).toEqual([promote]);
    expect(queueOpApplied("remove", { removed: true })).toBe(true);
    expect(queueOpApplied("promote", { promoted: false })).toBe(false);
    expect(queueOpApplied("promote", { removed: true })).toBe(false);
    expect(queueOpApplied("remove", undefined)).toBe(false);
  });
});
