/** L1: the pure layout edits behind layout editing and the widget picker (UI-103, UI-104). */

import { describe, expect, test } from "bun:test";
import { addWidget, hideNode, manifestToWrite, moveNode, RESIZE_STEP, resizeCells } from "../../src/view/edit.ts";
import type { LayoutNode } from "../../src/view/layout.ts";
import { DEFAULT_VIEW } from "../../src/view/views.ts";

const LAYOUT: LayoutNode = [
  "row",
  { sizes: [3, 1] },
  ["col", {}, "omfp:header", "omfp:chat"],
  ["tabs", { note: "kept" }, "omfp:workspace", ["./widgets/tests", { instance: "t1" }]],
];

/** The right-hand tabs of the default view. */
const SIDE_TABS: LayoutNode = ["tabs", { id: "side" }, "omfp:workspace", "omfp:changes", "omfp:background"];
/** The default view's `main` tabs, holding the chat. */
const MAIN_TABS: LayoutNode = ["tabs", { id: "main" }, "omfp:chat"];

describe("move", () => {
  // spec: UI-103
  test("before or after another widget; a container left with one child stays, its sizes follow", () => {
    expect(moveNode(DEFAULT_VIEW.layout, "0.1", "before", "0.0.0")).toEqual({
      ok: true,
      value: ["row", { sizes: [3] }, ["col", {}, SIDE_TABS, "omfp:header", MAIN_TABS]],
    });
    expect(moveNode(DEFAULT_VIEW.layout, "0.1.0", "before", "0.0.0")).toEqual({
      ok: true,
      value: [
        "row",
        { sizes: [3, 1] },
        ["col", {}, "omfp:workspace", "omfp:header", MAIN_TABS],
        ["tabs", { id: "side" }, "omfp:changes", "omfp:background"],
      ],
    });
    expect(moveNode(LAYOUT, "0.0.0", "after", "0.1.1")).toEqual({
      ok: true,
      value: [
        "row",
        { sizes: [3, 1] },
        ["col", {}, "omfp:chat"],
        ["tabs", { note: "kept" }, "omfp:workspace", ["./widgets/tests", { instance: "t1" }], "omfp:header"],
      ],
    });
    // Within one container the moved cell keeps its weight.
    expect(moveNode(["row", { sizes: [1, 2, 3] }, "a", "b", "c"], "0.0", "after", "0.2")).toEqual({
      ok: true,
      value: ["row", { sizes: [2, 3, 1] }, "b", "c", "a"],
    });
  });

  // spec: UI-103
  test("to the end of a container; an emptied container goes; a new cell weighs the mean", () => {
    expect(
      moveNode(["row", {}, ["col", {}, "a"], ["col", { sizes: [1, 3] }, "b", "c"]], "0.0.0", "end", "0.1"),
    ).toEqual({ ok: true, value: ["row", {}, ["col", { sizes: [1, 3, 2] }, "b", "c", "a"]] });
    expect(moveNode(["row", { sizes: [1, 1] }, ["col", {}, "a"], "b"], "0.0.0", "end", "0")).toEqual({
      ok: true,
      value: ["row", { sizes: [1, 1] }, "b", "a"],
    });
  });

  // spec: UI-103
  test("refuses the root, a missing node, a widget as container, and a node into itself", () => {
    expect(moveNode(LAYOUT, "0", "end", "0.0")).toEqual({ ok: false, error: "The view's root cannot move" });
    expect(moveNode(LAYOUT, "0.4", "end", "0")).toEqual({ ok: false, error: "No layout node at 0.4" });
    expect(moveNode(LAYOUT, "0.0.0", "end", "0.1.0")).toEqual({ ok: false, error: "0.1.0 is not a container" });
    expect(moveNode(LAYOUT, "0.0", "end", "0.0")).toEqual({ ok: false, error: "A node cannot move into itself" });
    expect(moveNode(LAYOUT, "0.0", "before", "0.0.1")).toEqual({ ok: false, error: "A node cannot move into itself" });
    expect(moveNode(LAYOUT, "0.0.0", "before", "0")).toEqual({ ok: false, error: "The view's root cannot move" });
  });
});

describe("hide", () => {
  // spec: UI-103
  test("removes the widget and its size; an emptied container goes too", () => {
    expect(hideNode(LAYOUT, "0.1.1")).toEqual({
      ok: true,
      value: [
        "row",
        { sizes: [3, 1] },
        ["col", {}, "omfp:header", "omfp:chat"],
        ["tabs", { note: "kept" }, "omfp:workspace"],
      ],
    });
    expect(hideNode(["row", { sizes: [2, 1] }, ["col", {}, "a"], "b"], "0.0.0")).toEqual({
      ok: true,
      value: ["row", { sizes: [1] }, "b"],
    });
  });

  // spec: UI-103
  test("the view's last widget cannot be hidden", () => {
    expect(hideNode("omfp:chat", "0")).toEqual({ ok: false, error: "The view's last widget cannot be hidden" });
    expect(hideNode(["row", {}, ["col", {}, "a"]], "0.0.0")).toEqual({
      ok: false,
      error: "The view's last widget cannot be hidden",
    });
    expect(hideNode(LAYOUT, "0.2")).toEqual({ ok: false, error: "No layout node at 0.2" });
  });
});

describe("resize", () => {
  // spec: UI-103
  test("a keyboard step moves 5 % of the container between two adjacent cells", () => {
    expect(resizeCells(DEFAULT_VIEW.layout, "0", 1, RESIZE_STEP)).toEqual({
      ok: true,
      value: ["row", { sizes: [3.2, 0.8] }, ["col", {}, "omfp:header", MAIN_TABS], SIDE_TABS],
    });
    expect(resizeCells(["col", {}, "a", "b", "c"], "0", 2, -RESIZE_STEP)).toEqual({
      ok: true,
      value: ["col", { sizes: [1, 0.85, 1.15] }, "a", "b", "c"],
    });
  });

  // spec: UI-103
  test("pointer deltas resize too, each cell keeping at least 5 %; only row and col resize", () => {
    expect(resizeCells(["row", { sizes: [1, 1] }, "a", "b"], "0", 1, 0.25)).toEqual({
      ok: true,
      value: ["row", { sizes: [1.5, 0.5] }, "a", "b"],
    });
    expect(resizeCells(["row", { sizes: [1, 1] }, "a", "b"], "0", 1, 2)).toEqual({
      ok: true,
      value: ["row", { sizes: [1.9, 0.1] }, "a", "b"],
    });
    expect(resizeCells(LAYOUT, "0.1", 1, RESIZE_STEP)).toEqual({ ok: false, error: "0.1 is not a container" });
    expect(resizeCells(LAYOUT, "0", 0, RESIZE_STEP)).toEqual({ ok: false, error: "No splitter at 0 before cell 0" });
    expect(resizeCells(LAYOUT, "0", 2, RESIZE_STEP)).toEqual({ ok: false, error: "No splitter at 0 before cell 2" });
  });
});

describe("add", () => {
  // spec: UI-104
  test("adds at the end of the root container; a root widget becomes a row of both", () => {
    expect(addWidget(DEFAULT_VIEW.layout, "user:charts")).toEqual([
      "row",
      { sizes: [3, 1, 2] },
      ["col", {}, "omfp:header", MAIN_TABS],
      SIDE_TABS,
      "user:charts",
    ]);
    expect(addWidget(["tabs", {}, "a"], "./widgets/tests")).toEqual(["tabs", {}, "a", "./widgets/tests"]);
    expect(addWidget(["omfp:chat", { instance: "x" }], "omfp:header")).toEqual([
      "row",
      {},
      ["omfp:chat", { instance: "x" }],
      "omfp:header",
    ]);
  });
});

describe("manifest to write", () => {
  // spec: UI-103, UI-104
  test("without views.json: a new manifest holding the default view with the change", () => {
    expect(
      manifestToWrite({ manifest: null, error: null }, "default", { kind: "add", reference: "omfp:transcript" }),
    ).toEqual({
      ok: true,
      value: {
        $schema: "omfp://schemas/views.json",
        default: "default",
        views: {
          default: {
            title: "Default",
            layout: ["row", { sizes: [3, 1, 2] }, ["col", {}, "omfp:header", MAIN_TABS], SIDE_TABS, "omfp:transcript"],
          },
        },
      },
    });
  });

  // spec: UI-103
  test("with views.json: that view's layout changes, everything else stays; an invalid file is never overwritten", () => {
    const manifest = {
      default: "work",
      comment: "kept",
      views: {
        review: { title: "Review", layout: "omfp:transcript" },
        work: { title: "Work", layout: ["row", {}, "omfp:chat", "omfp:workspace"] },
      },
    };
    expect(manifestToWrite({ manifest }, "work", { kind: "hide", path: "0.1" })).toEqual({
      ok: true,
      value: { ...manifest, views: { ...manifest.views, work: { title: "Work", layout: ["row", {}, "omfp:chat"] } } },
    });
    expect(manifestToWrite({ manifest }, "work", { kind: "resize", path: "0", index: 1, delta: RESIZE_STEP })).toEqual({
      ok: true,
      value: {
        ...manifest,
        views: {
          ...manifest.views,
          work: { title: "Work", layout: ["row", { sizes: [1.1, 0.9] }, "omfp:chat", "omfp:workspace"] },
        },
      },
    });
    expect(manifestToWrite({ manifest }, "review", { kind: "hide", path: "0" })).toEqual({
      ok: false,
      error: "The view's last widget cannot be hidden",
    });
    expect(manifestToWrite({ manifest }, "gone", { kind: "hide", path: "0" })).toEqual({
      ok: false,
      error: "No view gone",
    });
    expect(
      manifestToWrite({ manifest: null, error: "views: no view" }, "default", { kind: "add", reference: "omfp:chat" }),
    ).toEqual({ ok: false, error: "The layout cannot change while .omfp/views.json is invalid: fix the file first" });
  });
});
