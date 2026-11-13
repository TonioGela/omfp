/** L0: the views a window chose per thread and per workspace (UI-367). */

import { describe, expect, test } from "bun:test";
import {
  activeView,
  adoptView,
  chooseView,
  decodeViewChoices,
  keepListed,
  NO_VIEW_CHOICES,
  VIEW_CHOICE_THREADS,
  type ViewChoices,
} from "../../src/vm/view-choices.ts";

const VIEWS = ["work", "review", "lean"];

describe("view choices", () => {
  // spec: UI-367
  test("a thread's own view, else its workspace's last chosen, else the default; views gone are skipped", () => {
    let choices = chooseView(NO_VIEW_CHOICES, "t1", "w1", "review");
    choices = chooseView(choices, "t2", "w1", "lean");
    choices = chooseView(choices, undefined, "w2", "work");
    expect(activeView(choices, "t1", "w1", VIEWS)).toBe("review");
    expect(activeView(choices, "t2", "w1", VIEWS)).toBe("lean");
    expect(activeView(choices, "t3", "w1", VIEWS)).toBe("lean");
    expect(activeView(choices, undefined, "w2", VIEWS)).toBe("work");
    expect(activeView(choices, "t1", "w1", ["work", "lean"])).toBe("lean");
    expect(activeView(choices, "t1", "w1", ["work"])).toBeUndefined();
    expect(activeView(choices, undefined, "w3", VIEWS)).toBeUndefined();
    expect(activeView(choices, undefined, "constructor", VIEWS)).toBeUndefined();
    expect(activeView(choices, undefined, undefined, VIEWS)).toBeUndefined();
  });

  // spec: UI-367
  test("choosing again moves the thread last; the oldest threads go over the bound; no change keeps the object", () => {
    let choices: ViewChoices = NO_VIEW_CHOICES;
    for (let index = 0; index <= VIEW_CHOICE_THREADS; index += 1) {
      choices = chooseView(choices, `t${index}`, "w1", "work");
    }
    expect(choices.threads.length).toBe(VIEW_CHOICE_THREADS);
    expect(choices.threads[0]?.threadId).toBe("t1");
    choices = chooseView(choices, "t1", "w1", "lean");
    expect(choices.threads.at(-1)).toEqual({ threadId: "t1", viewId: "lean" });
    expect(choices.threads.length).toBe(VIEW_CHOICE_THREADS);
    expect(chooseView(choices, "t1", "w1", "lean")).toBe(choices);
    expect(chooseView(choices, undefined, undefined, "lean")).toBe(choices);
  });

  // spec: UI-367
  test("a thread shown for the first time adopts its workspace's last view, then keeps it", () => {
    const choices = chooseView(NO_VIEW_CHOICES, "t1", "w1", "review");
    const adopted = adoptView(choices, "t2", "w1");
    expect(adopted.threads).toEqual([
      { threadId: "t1", viewId: "review" },
      { threadId: "t2", viewId: "review" },
    ]);
    expect(adoptView(adopted, "t2", "w1")).toBe(adopted);
    expect(adoptView(adopted, "t3", "w2")).toBe(adopted);
    expect(adoptView(adopted, undefined, "w1")).toBe(adopted);
    expect(activeView(chooseView(adopted, "t1", "w1", "lean"), "t2", "w1", VIEWS)).toBe("review");
  });

  // spec: UI-367
  test("threads and workspaces the index no longer lists are dropped", () => {
    let choices = chooseView(NO_VIEW_CHOICES, "t1", "w1", "review");
    choices = chooseView(choices, "t2", "w2", "lean");
    expect(keepListed(choices, new Set(["t1", "t2"]), new Set(["w1", "w2"]))).toBe(choices);
    expect(keepListed(choices, new Set(["t2"]), new Set(["w2"]))).toEqual({
      threads: [{ threadId: "t2", viewId: "lean" }],
      workspaces: { w2: "lean" },
    });
  });

  // spec: UI-367
  test("decoding skips malformed entries and keeps the newest threads", () => {
    expect(decodeViewChoices("review")).toBeUndefined();
    expect(decodeViewChoices(null)).toBeUndefined();
    expect(decodeViewChoices([])).toBeUndefined();
    expect(
      decodeViewChoices({
        threads: [{ threadId: "t1", viewId: "review" }, { threadId: 2, viewId: "x" }, null, "t3"],
        workspaces: { w1: "lean", w2: 3 },
      }),
    ).toEqual({ threads: [{ threadId: "t1", viewId: "review" }], workspaces: { w1: "lean" } });
    expect(decodeViewChoices({ threads: "x", workspaces: [] })).toEqual(NO_VIEW_CHOICES);
    const many = Array.from({ length: VIEW_CHOICE_THREADS + 5 }, (_, index) => ({
      threadId: `t${index}`,
      viewId: "v",
    }));
    const decoded = decodeViewChoices({ threads: many, workspaces: {} });
    expect([decoded?.threads.length, decoded?.threads[0]?.threadId]).toEqual([VIEW_CHOICE_THREADS, "t5"]);
  });
});
