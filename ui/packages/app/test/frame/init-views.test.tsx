/** L4: views init files make and TSX views in the window (UI-267..UI-269, experimental). */

import { afterEach, describe, expect, test } from "bun:test";
import type { CallMethod } from "@omfp/client";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Frame } from "../../src/frame/frame.tsx";
import { initUrl } from "../../src/frame/init.tsx";
import { LAYOUT_EDIT } from "../../src/frame/layout.tsx";
import {
  ACTIVE_VIEW,
  Col,
  type InitApi,
  openFile,
  Row,
  SELECTED_THREAD,
  Tabs,
  Widget,
  type WidgetComponent,
  type WidgetProps,
} from "../../src/sdk/index.ts";
import { viewFileUrl } from "../../src/view/file-view.tsx";
import type { WidgetRegistry } from "../../src/view/resolve.ts";
import { PANE_SIZES } from "../../src/view/sizes.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";
import { renderWithHost, type TestHost, testHost } from "../support/host.tsx";

afterEach(() => {
  cleanup();
  document.head.innerHTML = "";
});

const stub =
  (name: string): WidgetComponent =>
  (props: WidgetProps) => <p data-testid="stub">{typeof props.repo === "string" ? `${name} ${props.repo}` : name}</p>;

const registry: WidgetRegistry = new Map<string, WidgetComponent>(
  ["header", "chat", "workspace", "changes", "background", "mcp", "file"].map((name) => [`omfp:${name}`, stub(name)]),
);

const BASE = "/widgets/test";
const WORKSPACE_URL = initUrl(BASE, "w1", "v1");
const USER_URL = initUrl(BASE, "user", "u1");
const opsUrl = (version: string) => viewFileUrl(BASE, "w1", version, "./views/ops.tsx");

/** The example of the docs (UI-268): the same layout as {@link SAME}. */
function Ops() {
  return (
    <Row sizes={[3, 2]}>
      <Tabs id="main">
        <Widget use="omfp:chat" />
      </Tabs>
      <Col sizes={[1, 1]}>
        <Widget use="omfp:changes" props={{ repo: "api" }} />
        <Tabs>
          <Widget use="omfp:background" />
          <Widget use="omfp:mcp" />
        </Tabs>
      </Col>
    </Row>
  );
}

const SAME = [
  "row",
  { sizes: [3, 2] },
  ["tabs", { id: "main" }, "omfp:chat"],
  ["col", { sizes: [1, 1] }, ["omfp:changes", { repo: "api" }], ["tabs", {}, "omfp:background", "omfp:mcp"]],
];

const MANIFEST = {
  default: "work",
  views: {
    ops: { title: "Ops", view: "./views/ops.tsx" },
    same: { title: "Same", layout: SAME },
    work: { title: "Work", layout: ["row", {}, ["tabs", { id: "main" }, "omfp:chat"], "omfp:workspace"] },
  },
};

// The style of the docs: the views change in place, then `use` shows them.
const workspaceInit = (omfp: InitApi) => {
  const views = omfp.views.load("views.json");
  views.add("review", {
    title: "Review",
    layout: ["row", {}, "omfp:chat", ...omfp.workspace.repos.map((r) => ["omfp:changes", { repo: r.name }] as const)],
  });
  omfp.views.use(views);
};

function setup(
  modules: Record<string, unknown>,
  { manifest = MANIFEST as unknown, user = null as string | null } = {},
) {
  const base = testHost(undefined, {
    importer: (url) => (url in modules ? Promise.resolve(modules[url]) : new Promise<unknown>(() => {})),
  });
  for (const method of ["widgets.report", "views.set"] as const satisfies readonly CallMethod[]) {
    base.client.answers.set(method, () => ({}));
  }
  const test = renderWithHost(<Frame registry={registry} now={() => 0} />, base);
  test.client.connected();
  test.client.index({
    workspaces: [workspace("w1", [repo("w1", "api", "main")])],
    threads: [thread("t1", "c", "w1")],
    init: user,
  });
  act(() => test.host.window.set(SELECTED_THREAD, "t1"));
  test.client.views("w1", { manifest, viewFiles: "f1" });
  test.flush();
  return test;
}

/** Lets imports, init runs and the renders they cause settle. */
async function settle(test: TestHost): Promise<void> {
  for (let round = 0; round < 4; round += 1) {
    await act(async () => {
      await test.host.init.settled();
      const { promise, resolve } = Promise.withResolvers<void>();
      setTimeout(resolve, 0);
      await promise;
    });
    test.flush();
  }
}

function show(test: TestHost, id: string): void {
  act(() => test.host.window.set(ACTIVE_VIEW, id));
}

/** The view as rendered: each laid-out element's path, kind and slot, then the widgets' text. */
function structure(): string[] {
  const view = screen.getByTestId("view");
  return [
    ...[...view.querySelectorAll<HTMLElement>("[data-path][data-layout]")].map(
      (element) => `${element.dataset.path} ${element.dataset.layout} ${element.dataset.slot ?? ""}`,
    ),
    ...within(view)
      .queryAllByTestId("stub")
      .map((element) => element.textContent ?? ""),
  ];
}

function splitter(path: string): HTMLElement {
  const found = screen.getAllByTestId("splitter").find((element) => element.dataset.path === path);
  if (found === undefined) {
    throw new Error(`no splitter at ${path}`);
  }
  return found;
}

function banner(): string | undefined {
  return screen.queryByTestId("view-file-error")?.textContent ?? undefined;
}

describe("TSX views", () => {
  // spec: UI-268
  test("a TSX view renders like the same layout in views.json: tree, main slot, splitters", async () => {
    const test = setup({ [opsUrl("f1")]: { default: Ops } });
    show(test, "same");
    await settle(test);
    const manifest = structure();
    show(test, "ops");
    await settle(test);
    expect(screen.getByTestId("view").dataset.view).toBe("ops");
    expect(structure()).toEqual(manifest);
    expect(screen.getAllByTestId("splitter").map((element) => element.dataset.path)).toEqual(["0", "0.1"]);
    // Files open as tabs of its `main` slot.
    act(() => openFile(test.host, "src/lib.rs"));
    test.flush();
    expect(within(screen.getByTestId("view")).getAllByTestId("file-tab").length).toBe(1);
    expect(banner()).toBeUndefined();
  });

  // spec: UI-268, UI-269
  test("its splitters keep this window's sizes by workspace, view and path; a double click resets; Edit layout is off", async () => {
    const test = setup({ [opsUrl("f1")]: { default: Ops } });
    show(test, "ops");
    await settle(test);
    expect(splitter("0").getAttribute("aria-valuenow")).toBe("60");
    fireEvent.keyDown(splitter("0"), { key: "ArrowRight" });
    expect(Object.keys(test.host.window.get(PANE_SIZES))).toEqual(["w1/ops/0"]);
    expect(splitter("0").getAttribute("aria-valuenow")).not.toBe("60");
    fireEvent.doubleClick(splitter("0"));
    expect(test.host.window.get(PANE_SIZES)).toEqual({});
    expect(splitter("0").getAttribute("aria-valuenow")).toBe("60");

    const edit = screen.getByTestId("layout-edit");
    expect(edit.hasAttribute("disabled") || edit.getAttribute("aria-disabled") === "true").toBe(true);
    expect(screen.getByTestId("layout-edit-note").textContent).toBe(
      "This view is .omfp/views/ops.tsx: edit that file to change its layout (splitter sizes still apply to this window).",
    );
    // Edit mode left on from another view does not apply to it.
    act(() => test.host.window.set(LAYOUT_EDIT, true));
    expect(screen.queryAllByTestId("widget-toolbar").length).toBe(0);
  });

  // spec: UI-268
  test("a compile error, a throw or a node that is no layout shows a banner and the default view; the next save recovers", async () => {
    const throws = () => {
      throw new Error("kaboom");
    };
    const noLayout = () => (
      <Row>
        <div />
      </Row>
    );
    const test = setup({
      [opsUrl("f1")]: { compileError: { file: "ops.tsx", line: 3, column: 5, message: "Unexpected token" } },
      [opsUrl("f2")]: { default: throws },
      [opsUrl("f3")]: { default: noLayout },
      [opsUrl("f4")]: { default: Ops },
    });
    show(test, "ops");
    await settle(test);
    expect(banner()).toBe("View .omfp/views/ops.tsx failed, showing the default view: ops.tsx:3:5: Unexpected token");
    expect(screen.getByTestId("view").dataset.view).toBe("default");
    expect(screen.getByTestId("view-switcher").textContent).toContain("Ops");

    test.client.viewsEvents("w1", { kind: "views_files", viewFiles: "f2" });
    test.flush();
    await settle(test);
    expect(banner()).toBe("View .omfp/views/ops.tsx failed, showing the default view: kaboom");
    expect(screen.getByTestId("view").dataset.view).toBe("default");

    test.client.viewsEvents("w1", { kind: "views_files", viewFiles: "f3" });
    test.flush();
    await settle(test);
    expect(banner()).toContain("<div> is not a layout component");

    test.client.viewsEvents("w1", { kind: "views_files", viewFiles: "f4" });
    test.flush();
    await settle(test);
    expect(banner()).toBeUndefined();
    expect(screen.getByTestId("view").dataset.view).toBe("ops");
  });
});

describe("views of init files in the window", () => {
  // spec: UI-267, UI-269
  test("a workspace's init file uses views.json's views plus a computed one; Edit layout writes only views.json's", async () => {
    const test = setup({ [WORKSPACE_URL]: { default: workspaceInit } });
    test.client.viewsEvents("w1", { kind: "views_init", init: "v1" });
    test.flush();
    await settle(test);
    const titles = () =>
      within(screen.getByTestId("view-switcher"))
        .getAllByRole("option", { hidden: true })
        .map((option) => option.textContent?.trim())
        .filter((title) => title !== "");
    expect(titles()).toEqual(["Ops", "Same", "Work", "Review"]);

    // views.json's view edits views.json as today.
    expect(screen.queryByTestId("layout-edit-note")).toBeNull();
    act(() => test.host.window.set(LAYOUT_EDIT, true));
    expect(screen.queryAllByTestId("widget-toolbar").length).toBeGreaterThan(0);

    // The computed view says where it comes from instead of writing a file the window ignores.
    show(test, "review");
    test.flush();
    expect(
      within(screen.getByTestId("view"))
        .getAllByTestId("stub")
        .map((element) => element.textContent),
    ).toEqual(["chat", "changes api"]);
    expect(screen.queryAllByTestId("widget-toolbar").length).toBe(0);
    expect(screen.getByTestId("layout-edit-note").textContent).toBe(
      "This view comes from the workspace's .omfp/init.ts: edit that file to change it (Edit layout writes only views.json's views).",
    );

    // A new repository runs the file again: the computed view follows.
    test.client.indexEvents({
      kind: "workspace_upserted",
      workspace: workspace("w1", [repo("w1", "api", "main"), repo("w1", "web", "main")]),
    });
    test.flush();
    await settle(test);
    expect(
      within(screen.getByTestId("view"))
        .getAllByTestId("stub")
        .map((element) => element.textContent),
    ).toEqual(["chat", "changes api", "changes web"]);

    // A new views.json runs it again with the new file.
    test.client.viewsEvents("w1", {
      kind: "views_manifest",
      manifest: { ...MANIFEST, views: { ...MANIFEST.views, extra: { title: "Extra", layout: "omfp:chat" } } },
      error: null,
    });
    test.flush();
    await settle(test);
    expect(titles()).toContain("Extra");
    expect(test.host.init.getSnapshot().views?.views.views.map((view) => view.id)).toEqual([
      "extra",
      "ops",
      "same",
      "work",
      "review",
    ]);
  });

  const DEFAULT_LAYOUT = [
    "row",
    {},
    ["col", {}, "omfp:header", ["tabs", { id: "main" }, "omfp:chat"]],
    ["tabs", { id: "side" }, "omfp:workspace", "omfp:changes", "omfp:background"],
  ];
  const EDITABLE = { default: "default", views: { default: { title: "Default", layout: DEFAULT_LAYOUT } } };

  /** The user's init file of the docs: the MCP servers as the last side tab of the default view. */
  const userInit = (omfp: InitApi) => {
    omfp.views.transform((views) => {
      views.update("default", (layout) => {
        layout.addTab("side", "omfp:mcp");
      });
    });
  };

  function toolbar(widget: string): HTMLElement {
    const found = screen.getAllByTestId("widget-toolbar").find((element) => element.dataset.widget === widget);
    if (found === undefined) {
      throw new Error(`no toolbar of ${widget}`);
    }
    return found;
  }

  function written(test: TestHost): unknown[] {
    return test.client.callsTo("views.set").map((call) => call.params);
  }

  // spec: UI-269
  test("a views.json view an init file transforms stays editable: edits write views.json, the file's nodes are marked and kept", async () => {
    const test = setup({ [USER_URL]: { default: userInit } }, { manifest: EDITABLE, user: "u1" });
    const user = userEvent.setup();
    await settle(test);
    const edit = screen.getByTestId("layout-edit");
    expect(edit.hasAttribute("disabled") || edit.getAttribute("aria-disabled") === "true").toBe(false);
    expect(screen.queryByTestId("layout-edit-note")).toBeNull();
    act(() => test.host.window.set(LAYOUT_EDIT, true));
    test.flush();

    // The MCP tab the user's file added carries the mark naming the file; no other tab does.
    const marks = screen.getAllByTestId("init-mark");
    expect(marks.map((mark) => [mark.dataset.file, mark.getAttribute("title")])).toEqual([
      ["user", "Added by the user's init.ts: edit that file to change or remove it"],
    ]);
    expect(marks[0]?.closest('[data-testid="view-tab"]') !== null).toBe(true);
    // It is no Move target of the other widgets.
    await user.click(within(toolbar("omfp:workspace")).getByTestId("widget-move"));
    const targets = screen.getAllByTestId("move-target").map((target) => target.dataset.path);
    expect(targets).toContain("0.1.2");
    expect(targets).not.toContain("0.1.3");
    await user.keyboard("{Escape}");

    // Hiding another widget writes views.json's layout, without the MCP tab.
    await user.click(within(toolbar("omfp:workspace")).getByTestId("widget-hide"));
    const hidden = [
      "row",
      {},
      ["col", {}, "omfp:header", ["tabs", { id: "main" }, "omfp:chat"]],
      ["tabs", { id: "side" }, "omfp:changes", "omfp:background"],
    ];
    expect(written(test)).toEqual([
      { workspaceId: "w1", manifest: { ...EDITABLE, views: { default: { title: "Default", layout: hidden } } } },
    ]);

    // The daemon pushes the new file back: the transform applies to it again.
    const pushed = { ...EDITABLE, views: { default: { title: "Default", layout: hidden } } };
    test.client.viewsEvents("w1", { kind: "views_manifest", manifest: pushed, error: null });
    test.flush();
    await settle(test);
    expect(test.host.init.getSnapshot().views?.manifest).toBe(pushed);
    expect(
      screen
        .getAllByTestId("view-tab")
        .slice(-3)
        .map((tab) => within(tab).queryByTestId("init-mark")?.dataset.file ?? "views.json"),
    ).toEqual(["views.json", "views.json", "user"]);

    // Moving a widget writes views.json's paths.
    await user.click(within(toolbar("omfp:header")).getByTestId("widget-move"));
    await user.click(
      screen
        .getAllByTestId("move-target")
        .find((target) => target.dataset.place === "after" && target.dataset.path === "0.1.1") ?? document.body,
    );
    expect(written(test)[1]).toEqual({
      workspaceId: "w1",
      manifest: {
        ...EDITABLE,
        views: {
          default: {
            title: "Default",
            layout: [
              "row",
              {},
              ["col", {}, ["tabs", { id: "main" }, "omfp:chat"]],
              ["tabs", { id: "side" }, "omfp:changes", "omfp:background", "omfp:header"],
            ],
          },
        },
      },
    });

    // The MCP tab's toolbar names the file to edit and has neither Move nor Hide.
    const mcpTab = screen.getAllByTestId("view-tab").find((tab) => within(tab).queryByTestId("init-mark") !== null);
    await user.click(mcpTab ?? document.body);
    const mcp = toolbar("omfp:mcp");
    expect(mcp.dataset.init).toBe("user");
    expect(within(mcp).queryByTestId("widget-hide")).toBeNull();
    expect(within(mcp).queryByTestId("widget-move")).toBeNull();
    expect(mcp.textContent).toContain("Added by the user's init.ts: edit that file to change or remove it");
  });
});
