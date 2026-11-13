/** L1: where the nodes of a views.json view init files changed come from, and its edits on views.json (UI-269). */

import { describe, expect, test } from "bun:test";
import { type LayoutNode, type LayoutTree, parseLayout } from "../../src/view/layout.ts";
import { nodeOrigins, toBase } from "../../src/view/origins.ts";

function tree(node: LayoutNode): LayoutTree {
  const parsed = parseLayout(node);
  if (!parsed.ok) {
    throw new Error(parsed.error);
  }
  return parsed.value;
}

const BASE = tree([
  "row",
  {},
  ["tabs", { id: "main" }, "omfp:chat"],
  ["tabs", { id: "side" }, "omfp:workspace", "omfp:changes"],
]);
/** The user's file adds the MCP tab. */
const USER = tree([
  "row",
  {},
  ["tabs", { id: "main" }, "omfp:chat"],
  ["tabs", { id: "side" }, "omfp:workspace", "omfp:changes", "omfp:mcp"],
]);
/** The workspace's file puts a header over the whole row and drops the workspace tab. */
const WORKSPACE = tree([
  "col",
  {},
  "omfp:header",
  ["row", {}, ["tabs", { id: "main" }, "omfp:chat"], ["tabs", { id: "side" }, "omfp:changes", "omfp:mcp"]],
]);

const origins = nodeOrigins(BASE, [
  { file: "user", tree: USER },
  { file: "workspace", tree: WORKSPACE },
]);

describe("node origins", () => {
  // spec: UI-269
  test("every shown node is views.json's at its path there, or the file's that added it, through every file", () => {
    expect(Object.fromEntries(origins.nodes)).toEqual({
      "0": { init: "workspace" },
      "0.0": { init: "workspace" },
      "0.1": { base: "0" },
      "0.1.0": { base: "0.0" },
      "0.1.0.0": { base: "0.0.0" },
      "0.1.1": { base: "0.1" },
      "0.1.1.0": { base: "0.1.1" },
      "0.1.1.1": { init: "user" },
    });
    // A node that became the root is still views.json's.
    expect(
      Object.fromEntries(
        nodeOrigins(BASE, [{ file: "user", tree: tree(["tabs", { id: "main" }, "omfp:chat"]) }]).nodes,
      ),
    ).toEqual({
      "0": { base: "0.0" },
      "0.0": { base: "0.0.0" },
    });
  });

  // spec: UI-269
  test("edits go to views.json's paths; nodes a file added cannot move, hide or take a move", () => {
    expect(toBase(origins, WORKSPACE, { kind: "hide", path: "0.1.1.0" })).toEqual({
      ok: true,
      value: { kind: "hide", path: "0.1.1" },
    });
    expect(toBase(origins, WORKSPACE, { kind: "move", from: "0.1.0.0", place: "after", to: "0.1.1.0" })).toEqual({
      ok: true,
      value: { kind: "move", from: "0.0.0", place: "after", to: "0.1.1" },
    });
    expect(toBase(origins, WORKSPACE, { kind: "add", reference: "omfp:todos" })).toEqual({
      ok: true,
      value: { kind: "add", reference: "omfp:todos" },
    });
    expect(toBase(origins, WORKSPACE, { kind: "hide", path: "0.1.1.1" })).toEqual({
      ok: false,
      error: "Added by the user's init.ts: edit that file to change or remove it",
    });
    expect(toBase(origins, WORKSPACE, { kind: "move", from: "0.1.0", place: "before", to: "0.0" })).toEqual({
      ok: false,
      error: "Added by the workspace's .omfp/init.ts: edit that file to change or remove it",
    });
    expect(toBase(origins, WORKSPACE, { kind: "hide", path: "0.9" })).toEqual({
      ok: false,
      error: "No layout node at 0.9",
    });
  });

  // spec: UI-269
  test("a splitter writes views.json's sizes only between cells that are all views.json's, as there", () => {
    expect(toBase(origins, WORKSPACE, { kind: "resize", path: "0.1", index: 1, delta: 0.05 })).toEqual({
      ok: true,
      value: { kind: "resize", path: "0", index: 1, delta: 0.05 },
    });
    expect(toBase(origins, WORKSPACE, { kind: "resize", path: "0", index: 1, delta: 0.05 })).toEqual({
      ok: false,
      error: "Added by the workspace's .omfp/init.ts: edit that file to change or remove it",
    });
    const three = tree(["row", {}, "a", "b", "c"]);
    const two = tree(["row", {}, "a", "c"]);
    expect(
      toBase(nodeOrigins(three, [{ file: "user", tree: two }]), two, {
        kind: "resize",
        path: "0",
        index: 1,
        delta: 0.1,
      }),
    ).toEqual({
      ok: false,
      error: "An init file changed these cells: resize them in that file",
    });
    const added = tree(["row", {}, "a", "b", "c", "d"]);
    expect(
      toBase(nodeOrigins(three, [{ file: "user", tree: added }]), added, {
        kind: "resize",
        path: "0",
        index: 1,
        delta: 0.1,
      }),
    ).toEqual({
      ok: false,
      error: "Added by the user's init.ts: edit that file to change or remove it",
    });
  });
});
