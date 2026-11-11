/** L0: the session tree's rows (UI-083), from the `get_tree` shape recorded from the pinned omp. */

import { describe, expect, test } from "bun:test";
import { treeView } from "../../src/vm/tree.ts";

/** A `get_tree` node; `type` and `extra` give the entry, `children` its nodes. */
function node(
  id: string,
  parentId: string | null,
  extra: Record<string, unknown>,
  children: unknown[] = [],
  label?: string,
) {
  return { entry: { id, parentId, timestamp: "t", ...extra }, children, ...(label === undefined ? {} : { label }) };
}

function message(
  id: string,
  parentId: string | null,
  role: string,
  content: unknown,
  children: unknown[] = [],
  label?: string,
) {
  return node(id, parentId, { type: "message", message: { role, content, timestamp: 0 } }, children, label);
}

/**
 * The tree of `contract_subagents_tree.rs` (model and thinking changes, a custom record, two turns) with a second
 * branch from the first answer, a tool call, its result and a branch summary.
 */
const TREE = {
  leafId: "label1",
  tree: [
    node("model", null, { type: "model_change", model: "fake/scripted" }, [
      node("thinking", "model", { type: "thinking_level_change", thinkingLevel: null }, [
        node("reminder", "thinking", { type: "custom", customType: "date-cwd-reminder" }, [
          message("u1", "reminder", "user", "first question\nwith a second line", [
            message(
              "a1",
              "u1",
              "assistant",
              [{ type: "text", text: "first answer" }],
              [
                message(
                  "u2",
                  "a1",
                  "user",
                  [{ type: "text", text: "second question" }],
                  [
                    message(
                      "a2",
                      "u2",
                      "assistant",
                      [{ type: "toolCall", id: "c1", name: "read", arguments: {} }],
                      [message("r2", "a2", "toolResult", [{ type: "text", text: "file" }])],
                    ),
                  ],
                ),
                node("s1", "a1", { type: "branch_summary", summary: "Tried the second question" }, [
                  message("u3", "s1", "user", "third question", [
                    node("label1", "u3", { type: "label", targetId: "a1", label: "checkpoint" }),
                  ]),
                ]),
              ],
              "checkpoint",
            ),
          ]),
        ]),
      ]),
    ]),
  ],
};

describe("treeView", () => {
  // spec: UI-083
  test("conversation entries only; a chain keeps its depth, branches are indented", () => {
    const { rows } = treeView(TREE);
    expect(rows.map((row) => [row.id, row.depth, row.kind, row.text])).toEqual([
      ["u1", 0, "user", "first question"],
      ["a1", 0, "assistant", "first answer"],
      ["u2", 1, "user", "second question"],
      ["a2", 1, "assistant", "→ read"],
      ["s1", 1, "summary", "Branch summary: Tried the second question"],
      ["u3", 1, "user", "third question"],
    ]);
  });

  // spec: UI-083
  test("the branch to the leaf is active, its deepest shown entry is the leaf; labels and forkable entries", () => {
    const { rows, leafId } = treeView(TREE);
    expect(leafId).toBe("label1");
    expect(rows.filter((row) => row.active).map((row) => row.id)).toEqual(["u1", "a1", "s1", "u3"]);
    expect(rows.filter((row) => row.leaf).map((row) => row.id)).toEqual(["u3"]);
    expect(rows.find((row) => row.id === "a1")?.label).toBe("checkpoint");
    expect(rows.filter((row) => !row.forkable).map((row) => row.id)).toEqual(["s1"]);
  });

  // spec: UI-083
  test("an empty or malformed result has no rows", () => {
    expect(treeView({ tree: [], leafId: null })).toEqual({ rows: [], leafId: undefined });
    expect(treeView(undefined).rows).toEqual([]);
    expect(treeView({ tree: [{ entry: {} }, 3] }).rows).toEqual([]);
  });
});
