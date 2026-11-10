/**
 * The TypeScript reducer reproduces the Rust reducer's state after every event of every recorded fixture: the golden
 * states rendered by `crates/omfp-core/tests/golden_states.rs` (`just gen-states`).
 */

import { describe, expect, test } from "bun:test";
import { emptyThreadState, reduce } from "../src/index.ts";
import { canonical, deepFreeze, FIXTURE_DIR, fixture, jsonlLines, jsonlNames, STATE_DIR } from "./helpers.ts";

/** Path and both sides of the first place where two JSON values differ. */
function firstDifference(actual: unknown, expected: unknown, path: string): string | undefined {
  if (canonical(actual) === canonical(expected)) {
    return undefined;
  }
  const bothArrays = Array.isArray(actual) && Array.isArray(expected);
  const bothObjects =
    typeof actual === "object" &&
    actual !== null &&
    !Array.isArray(actual) &&
    typeof expected === "object" &&
    expected !== null &&
    !Array.isArray(expected);
  if (bothArrays || bothObjects) {
    const left = actual as Record<string, unknown>;
    const right = expected as Record<string, unknown>;
    const keys = [...new Set([...Object.keys(left), ...Object.keys(right)])].sort();
    for (const key of keys) {
      const found = firstDifference(left[key], right[key], `${path}${bothArrays ? `[${key}]` : `.${key}`}`);
      if (found !== undefined) {
        return found;
      }
    }
  }
  const show = (value: unknown) => (value === undefined ? "<missing>" : JSON.stringify(value).slice(0, 400));
  return `${path}: expected ${show(expected)}, got ${show(actual)}`;
}

describe("golden states", () => {
  // spec: ARC-006, TST-012
  test("every fixture has a golden state file and no state file is stale", () => {
    expect(jsonlNames(STATE_DIR)).toEqual(jsonlNames(FIXTURE_DIR));
  });

  for (const name of jsonlNames(FIXTURE_DIR)) {
    // spec: ARC-006, TST-012
    test(`${name}: the state after every event equals the Rust reducer's`, () => {
      const events = fixture(name);
      const golden = jsonlLines(STATE_DIR, name);
      expect(golden.length).toBe(events.length);

      let state = emptyThreadState();
      const mismatches: string[] = [];
      for (const [index, event] of events.entries()) {
        // Frozen inputs: any mutation inside `reduce` throws.
        const previous = deepFreeze(state);
        const before = canonical(previous);
        state = reduce(previous, deepFreeze(event));
        expect(canonical(previous)).toBe(before);

        const expected: unknown = JSON.parse(golden[index] ?? "null");
        if (canonical(state) !== canonical(expected)) {
          const difference = firstDifference(state, expected, "state");
          mismatches.push(`${name} event ${index} (${JSON.stringify(event).slice(0, 120)}): ${difference}`);
          break;
        }
      }
      expect(mismatches).toEqual([]);
    });
  }
});
