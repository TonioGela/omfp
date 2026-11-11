/** The files the agent opens with `omfp_open_file` (DMN-060); port of the open-file test of `reducer.rs`. */

import { describe, expect, test } from "bun:test";
import { reduce, replay } from "../src/index.ts";
import { deepFreeze, event } from "./helpers.ts";

describe("open file requests", () => {
  // spec: DMN-060, TST-010
  test("the last request is kept with its window and survives the process", () => {
    const state = replay([
      event({ kind: "process", state: { state: "running" } }),
      event({ kind: "open_file", id: "o1", connection: 3, path: "src/a.rs", line: 12 }),
      event({ kind: "open_file", id: "o2", connection: 4, path: "b.md", line: null }),
      event({ kind: "process", state: { state: "stopped" } }),
    ]);

    expect(state.open_file).toEqual({ id: "o2", connection: 4, path: "b.md", line: null });
  });

  // spec: DMN-060
  test("a request that would not decode in Rust changes nothing", () => {
    const base = deepFreeze(replay([event({ kind: "open_file", id: "o1", connection: 3, path: "a.rs", line: 1 })]));

    for (const bad of [
      { kind: "open_file", id: "o2", connection: -1, path: "b.rs" },
      { kind: "open_file", id: "o2", connection: 3, path: 7 },
      { kind: "open_file", id: "o2", connection: 3, path: "b.rs", line: -2 },
    ]) {
      expect(reduce(base, event(bad))).toBe(base);
    }
    expect(reduce(base, event({ kind: "open_file", id: "o2", connection: 3, path: "b.rs" })).open_file).toEqual({
      id: "o2",
      connection: 3,
      path: "b.rs",
      line: null,
    });
  });
});
