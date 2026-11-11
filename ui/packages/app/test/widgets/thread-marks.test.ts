import { describe, expect, test } from "bun:test";
import { relativeAge } from "../../src/widgets/threads/age.ts";
import { threadMarkState } from "../../src/widgets/threads/marks.ts";

describe("left column rows", () => {
  // spec: UI-137, UI-169
  test("a thread's mark: running, awaiting and failed win; otherwise unread or idle", () => {
    expect(threadMarkState("running", true)).toBe("running");
    expect(threadMarkState("starting", false)).toBe("running");
    expect(threadMarkState("awaiting", true)).toBe("awaiting");
    expect(threadMarkState("failed", false)).toBe("failed");
    expect(threadMarkState("idle", true)).toBe("unread");
    expect(threadMarkState("stopped", false)).toBe("idle");
    expect(threadMarkState("terminal", false)).toBe("idle");
  });

  // spec: UI-169
  test("the row's time is how long ago, in the largest whole unit", () => {
    const now = 10 * 86_400_000;
    expect(relativeAge(now - 30_000, now)).toBe("now");
    expect(relativeAge(now + 5_000, now)).toBe("now");
    expect(relativeAge(now - 12 * 60_000, now)).toBe("12m");
    expect(relativeAge(now - 2 * 3_600_000 - 59 * 60_000, now)).toBe("2h");
    expect(relativeAge(now - 3 * 86_400_000, now)).toBe("3d");
  });
});
