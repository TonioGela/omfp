/** L0: the session tree's rows (UI-083), from the `get_tree` shape recorded from the pinned omp. */

import { describe, expect, test } from "bun:test";
import { messageEntryId, treeView } from "../../src/vm/tree.ts";

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

describe("the entry of a transcript message", () => {
  const entry = (id: string, parentId: string | null, role: string, timestamp: number, children: unknown[] = []) =>
    node(id, parentId, { type: "message", message: { role, content: id, timestamp } }, children);
  // u1 → a1 → (u2 → a2, the leaf) and a1 → u2b, a branch with the same time stamp as u2.
  const tree = {
    leafId: "a2",
    tree: [
      entry("u1", null, "user", 1, [
        entry("a1", "u1", "assistant", 2, [
          node("l1", "a1", { type: "label" }, [entry("u2", "l1", "user", 3, [entry("a2", "u2", "assistant", 4)])]),
          entry("u2b", "a1", "user", 3),
        ]),
      ]),
    ],
  };

  // spec: UI-661
  test("is the message entry with that role and time stamp on the current branch, off-branch ones not", () => {
    expect(messageEntryId(tree, "user", 3)).toBe("u2");
    expect(messageEntryId(tree, "user", 1)).toBe("u1");
    expect(messageEntryId(tree, "assistant", 2)).toBe("a1");
    expect(messageEntryId(tree, "assistant", 3)).toBeUndefined();
    expect(messageEntryId({ ...tree, leafId: "u2b" }, "user", 3)).toBe("u2b");
    expect(messageEntryId({ ...tree, leafId: "gone" }, "user", 1)).toBeUndefined();
    expect(messageEntryId({ tree: [], leafId: null }, "user", 1)).toBeUndefined();
    expect(messageEntryId(undefined, "user", 1)).toBeUndefined();
  });
});
