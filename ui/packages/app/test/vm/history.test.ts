/** L0: the composer's prompt history walk (UI-361..UI-363). */

import { describe, expect, test } from "bun:test";
import type { Draft } from "../../src/sdk/index.ts";
import { historyDown, historyUp, onFirstLine, onLastLine } from "../../src/vm/history.ts";

const draft = (text: string): Draft => ({ text, images: [], pastes: [] });

describe("prompt history", () => {
  // spec: UI-361
  test("Up and Down only act with the caret on the first or last line and no selection", () => {
    expect(onFirstLine("one\ntwo", 2, 2)).toBe(true);
    expect(onFirstLine("one\ntwo", 5, 5)).toBe(false);
    expect(onFirstLine("one", 0, 2)).toBe(false);
    expect(onLastLine("one\ntwo", 5, 5)).toBe(true);
    expect(onLastLine("one\ntwo", 2, 2)).toBe(false);
    expect(onLastLine("", 0, 0)).toBe(true);
  });

  // spec: UI-361, UI-362
  test("Up walks older prompts, Down newer ones and past the newest restores the draft", () => {
    const typed: Draft = { text: "half typed", images: [], pastes: [{ id: 1, text: "x" }] };
    const first = historyUp(undefined, ["newest", "older"], typed);
    expect(first?.draft).toEqual(draft("newest"));
    const second = historyUp(first?.walk, [], draft("newest"));
    expect(second?.draft).toEqual(draft("older"));
    expect(historyUp(second?.walk, [], draft("older"))).toBeUndefined();
    const back = historyDown(second?.walk);
    expect(back?.draft).toEqual(draft("newest"));
    const restored = historyDown(back?.walk);
    expect(restored).toEqual({ walk: undefined, draft: typed });
    expect(historyDown(undefined)).toBeUndefined();
  });

  // spec: UI-361
  test("without prompts Up does nothing", () => {
    expect(historyUp(undefined, [], draft("x"))).toBeUndefined();
  });
});
