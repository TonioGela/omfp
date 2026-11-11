/** The notifications the agent sends with `omfp_notify` (DMN-125, PRT-065); port of the notify test of `reducer.rs`. */

import { describe, expect, test } from "bun:test";
import { reduce, replay } from "../src/index.ts";
import { deepFreeze, event } from "./helpers.ts";

describe("notify requests", () => {
  // spec: DMN-125, PRT-065, TST-010
  test("the last request is kept with its window and survives the process", () => {
    const state = replay([
      event({ kind: "process", state: { state: "running" } }),
      event({ kind: "notify", id: "n1", connection: 3, title: "Tests", body: "All green" }),
      event({ kind: "notify", id: "n2", connection: 4, title: "Deploy", body: "Needs your OK" }),
      event({ kind: "process", state: { state: "stopped" } }),
    ]);

    expect(state.notify).toEqual({ id: "n2", connection: 4, title: "Deploy", body: "Needs your OK" });
    expect(state.open_file).toBeNull();
  });

  // spec: PRT-065
  test("a request that would not decode in Rust changes nothing", () => {
    const base = deepFreeze(replay([event({ kind: "notify", id: "n1", connection: 3, title: "Tests", body: "Done" })]));

    for (const bad of [
      { kind: "notify", id: "n2", connection: -1, title: "Tests", body: "Done" },
      { kind: "notify", id: "n2", connection: 3, title: 7, body: "Done" },
      { kind: "notify", id: "n2", connection: 3, title: "Tests" },
      { kind: "notify", connection: 3, title: "Tests", body: "Done" },
    ]) {
      expect(reduce(base, event(bad))).toBe(base);
    }
  });
});
