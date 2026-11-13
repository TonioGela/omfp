/** L1: the window state the window reports for agents (UI-350, UI-351, PRT-122). */

import { describe, expect, test } from "bun:test";
import { widgetsOf, windowReport } from "../../src/frame/window-state.ts";
import type { Binding } from "../../src/keymap/keymap.ts";
import type { Command } from "../../src/sdk/index.ts";
import { parseLayout } from "../../src/view/layout.ts";
import { BUILTIN_VIEW, type FileView, type ResolvedView, type Views } from "../../src/view/views.ts";

function resolved(id: string, layout: unknown, extra: Partial<ResolvedView> = {}): ResolvedView {
  const tree = parseLayout(layout);
  if (!tree.ok) {
    throw new Error(tree.error);
  }
  return { id, title: id.toUpperCase(), layout: layout as ResolvedView["layout"], tree: tree.value, ...extra };
}

const work = resolved("work", ["row", {}, ["tabs", { id: "main" }, "omfp:chat"], "./widgets/tests", "omfp:chat"]);
const solo = resolved("solo", "omfp:chat", { init: "workspace" });
const ops: FileView = { id: "ops", title: "Ops", file: "./views/ops.tsx", init: "user" };
const views: Views = { views: [ops, solo, work], defaultId: "work", panels: [], error: undefined };

const noop = () => {};
const commands: Command[] = [
  { id: "view.switch", title: "Switch view", aliases: ["view"], run: noop },
  { id: "demo.solo", title: "Show solo", init: "workspace", aliases: ["solo"], run: noop },
  { id: "tests.run", title: "Run tests", widget: "./widgets/tests", enabled: () => false, run: noop },
];
const bindings: Binding[] = [
  { mode: "normal", keys: "g v", command: "view.pick" },
  { mode: "normal", keys: "g h", command: "demo.solo", source: "init:workspace" },
  { mode: "insert", keys: "Ctrl-t", command: "tests.run", args: ["unit"], source: "workspace" },
];

describe("window state", () => {
  // spec: UI-350
  test("each view says where it comes from, its TSX file and its widgets; a TSX view only once rendered", () => {
    const report = windowReport({
      workspaceId: "w1",
      threadId: "t1",
      focused: true,
      views,
      listed: work,
      shown: work,
      commands: [],
      bindings,
    });
    expect(report).toEqual({
      threadId: "t1",
      focused: true,
      activeView: "work",
      defaultView: "work",
      views: [
        { id: "ops", title: "Ops", source: "user_init", file: "./views/ops.tsx", widgets: null },
        { id: "solo", title: "SOLO", source: "workspace_init", file: null, widgets: ["omfp:chat"] },
        { id: "work", title: "WORK", source: "views_json", file: null, widgets: ["omfp:chat", "./widgets/tests"] },
      ],
      commands: [],
    });

    // The TSX view rendered: its layout's widgets; failing, the default view stands in and says nothing of it.
    const rendered = resolved("ops", ["row", {}, "omfp:chat", "./widgets/tests"], { file: "./views/ops.tsx" });
    const shown = windowReport({
      workspaceId: "w1",
      threadId: undefined,
      focused: false,
      views,
      listed: ops,
      shown: rendered,
      commands: [],
      bindings,
    });
    expect(shown.activeView).toBe("ops");
    expect(shown.views?.[0]?.widgets).toEqual(["omfp:chat", "./widgets/tests"]);
    const failing = windowReport({
      workspaceId: "w1",
      threadId: undefined,
      focused: false,
      views,
      listed: ops,
      shown: BUILTIN_VIEW,
      commands: [],
      bindings,
    });
    expect(failing.views?.[0]?.widgets).toBeNull();

    const builtin: Views = { views: [BUILTIN_VIEW], defaultId: "default", panels: [], error: undefined };
    const fallback = windowReport({
      workspaceId: "w1",
      threadId: undefined,
      focused: false,
      views: builtin,
      listed: BUILTIN_VIEW,
      shown: BUILTIN_VIEW,
      commands: [],
      bindings,
    });
    expect(fallback.views?.map((view) => view.source)).toEqual(["builtin"]);
  });

  // spec: UI-351
  test("only commands of init files and widgets are reported, with who registered them and their keys", () => {
    const report = windowReport({
      workspaceId: "w1",
      threadId: undefined,
      focused: false,
      views,
      listed: work,
      shown: work,
      commands,
      bindings,
    });
    expect(report.commands).toEqual([
      {
        id: "demo.solo",
        title: "Show solo",
        aliases: ["solo"],
        source: "workspace_init",
        widget: null,
        enabled: true,
        bindings: [{ mode: "normal", keys: "g h", args: [] }],
      },
      {
        id: "tests.run",
        title: "Run tests",
        aliases: [],
        source: "widget",
        widget: "./widgets/tests",
        enabled: false,
        bindings: [{ mode: "insert", keys: "Ctrl-t", args: ["unit"] }],
      },
    ]);
  });

  // spec: UI-350
  test("a window without a workspace reports no views", () => {
    expect(
      windowReport({
        workspaceId: undefined,
        threadId: undefined,
        focused: true,
        views,
        listed: work,
        shown: work,
        commands: [],
        bindings,
      }),
    ).toEqual({ threadId: null, focused: true, activeView: null, defaultView: null, views: [], commands: [] });
  });

  // spec: UI-350
  test("a layout's widgets are listed once each, in layout order", () => {
    expect(widgetsOf(work.tree)).toEqual(["omfp:chat", "./widgets/tests"]);
  });
});
