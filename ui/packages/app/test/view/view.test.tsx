/** L1/L4: view layouts, their validation and the renderer with its error boundaries (UI-024, AGT-010, AGT-011). */

import { afterEach, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { OmfpProvider, type WidgetComponent } from "../../src/sdk/index.ts";
import type { LayoutEdit } from "../../src/view/edit.ts";
import { containsWidget, parseLayout } from "../../src/view/layout.ts";
import { ViewRenderer } from "../../src/view/render.tsx";
import type { WidgetRegistry } from "../../src/view/resolve.ts";
import { DEFAULT_MANIFEST, DEFAULT_VIEW, isFileView, type ResolvedView, resolveViews } from "../../src/view/views.ts";
import { renderWithHost } from "../support/host.tsx";

afterEach(cleanup);

const Named: WidgetComponent = (props) => <span data-testid="named">{String(props.label ?? "plain")}</span>;
const Broken: WidgetComponent = () => {
  throw new Error("boom");
};

const registry: WidgetRegistry = new Map([
  ["omfp:named", Named],
  ["omfp:broken", Broken],
]);

function widgets(): string[] {
  return screen.getAllByTestId("widget").map((element) => element.dataset.widget ?? "");
}

interface ManifestCase {
  readonly name: string;
  readonly manifest: unknown;
  readonly error: string | null;
}

// The shared fixture of the repository (`fixtures/views/cases.json`), whose shape the daemon's test checks too.
const FIXTURE = JSON.parse(readFileSync(join(import.meta.dir, "../../../../../fixtures/views/cases.json"), "utf8")) as {
  readonly cases: readonly ManifestCase[];
};
const CASES = FIXTURE.cases;

describe("layouts", () => {
  // spec: UI-024, AGT-010
  test("the default view is header over the main tabs with the chat, the side tabs on the right; the threads column is the frame's", () => {
    const { views, defaultId, error } = resolveViews(undefined);
    expect([defaultId, error, views.length]).toEqual(["default", undefined, 1]);
    expect(DEFAULT_VIEW.layout).toEqual([
      "row",
      { sizes: [3, 1] },
      ["col", {}, "omfp:header", ["tabs", { id: "main" }, "omfp:chat"]],
      ["tabs", { id: "side" }, "omfp:workspace", "omfp:changes", "omfp:background"],
    ]);
    expect(DEFAULT_VIEW.title).toBe("Default");
    const [first] = views;
    const tree = first === undefined || isFileView(first) ? undefined : first.tree;
    expect(tree !== undefined && containsWidget(tree, ["omfp:chat"])).toBe(true);
    expect(tree !== undefined && containsWidget(tree, ["omfp:threads", "omfp:interactions"])).toBe(false);
  });

  // spec: AGT-010, DMN-088
  test("the default manifest is the daemon's, which it writes into every workspace", () => {
    const daemon: unknown = JSON.parse(
      readFileSync(join(import.meta.dir, "../../../../../crates/omfp-daemon/src/widgets/default-views.json"), "utf8"),
    );
    expect(daemon).toEqual(DEFAULT_MANIFEST);
    expect(
      resolveViews(DEFAULT_MANIFEST).views.map((view) => [
        view.id,
        view.title,
        isFileView(view) ? view.file : view.layout,
      ]),
    ).toEqual([[DEFAULT_VIEW.id, DEFAULT_VIEW.title, DEFAULT_VIEW.layout]]);
  });

  // spec: AGT-010, AGT-011
  test("every shared manifest case gets the daemon's error, views listed in id order", () => {
    expect(CASES.length).toBeGreaterThan(10);
    for (const { name, manifest, error } of CASES) {
      const resolved = resolveViews(manifest);
      expect([name, resolved.error ?? null]).toEqual([name, error]);
      if (error !== null) {
        expect([name, resolved.views.map((view) => view.id)]).toEqual([name, ["default"]]);
      }
    }
    const everything = resolveViews(CASES.find((entry) => entry.name === "everything")?.manifest);
    expect(everything.views.map((view) => [view.id, view.title])).toEqual([
      ["review", "Review"],
      ["work", "Work"],
    ]);
    expect(everything.defaultId).toBe("work");
    const ordered = resolveViews({ views: { zeta: { layout: "omfp:chat" }, alpha: { layout: "omfp:header" } } });
    expect([ordered.views.map((view) => view.id), ordered.defaultId]).toEqual([["alpha", "zeta"], "alpha"]);
    expect(resolveViews({ views: { work: { title: 3, layout: "omfp:chat" } } }).views[0]?.title).toBe("work");
  });

  // spec: UI-024
  test("widget references with props and nested containers parse; mistakes name their place", () => {
    expect(parseLayout(["omfp:changes", { repo: "x" }])).toEqual({
      ok: true,
      value: { kind: "widget", name: "omfp:changes", props: { repo: "x" } },
    });
    expect(parseLayout(["tabs", {}, "a", ["stack", {}, "b"]]).ok).toBe(true);
    expect(parseLayout(42)).toEqual({
      ok: false,
      error: "layout: expected a widget name, [widget, props] or [container, props, ...children]",
    });
    expect(parseLayout(["row", {}])).toEqual({ ok: false, error: "layout: row has no children" });
    expect(parseLayout(["row", "a"])).toEqual({ ok: false, error: "layout: row needs a props object" });
    expect(parseLayout(["row", { sizes: [1] }, "a", "b"])).toEqual({
      ok: false,
      error: "layout: sizes must be 2 positive numbers",
    });
    expect(parseLayout(["col", {}, "a", ["row", {}, ""]])).toEqual({
      ok: false,
      error: "layout[3][2]: a widget name is empty",
    });
    expect(parseLayout(["omfp:x", {}, "extra"])).toEqual({
      ok: false,
      error: "layout: a widget reference is [name, props]",
    });
  });

  // spec: UI-024
  test("an invalid manifest falls back to the default view with its error", () => {
    const invalid = resolveViews({ views: { work: { title: "Work", layout: ["row", {}] } } });
    expect([invalid.defaultId, invalid.views.map((view) => view.id), invalid.error]).toEqual([
      "default",
      ["default"],
      "views.work.layout: row has no children",
    ]);
    expect(resolveViews({ default: "nope", views: { a: { layout: "x" } } }).error).toBe("default: no view nope");
    const valid = resolveViews({ default: "b", views: { a: { title: "A", layout: "x" }, b: { layout: "y" } } });
    expect([valid.defaultId, valid.views.map((view) => view.title), valid.error]).toEqual(["b", ["A", "b"], undefined]);
  });
});

function only(manifest: unknown): ResolvedView {
  const view = resolveViews(manifest).views[0];
  if (view === undefined || isFileView(view)) {
    throw new Error("fixture");
  }
  return view;
}

/** Renders `layout` as the view `v`, in edit mode when `edits` collects the changes. */
function show(layout: unknown, edits?: LayoutEdit[]) {
  const onEdit =
    edits === undefined
      ? undefined
      : async (edit: LayoutEdit) => {
          edits.push(edit);
          return true;
        };
  return renderWithHost(<ViewRenderer view={only({ views: { v: { layout } } })} registry={registry} onEdit={onEdit} />);
}

describe("renderer", () => {
  // spec: UI-024, AGT-010
  test("containers render their widgets in order with their props, sizes as flex weights", () => {
    show(["row", { sizes: [2, 1] }, ["omfp:named", { label: "first" }], ["col", {}, "omfp:named"]]);
    expect(screen.getByTestId("view").dataset.view).toBe("v");
    expect(widgets()).toEqual(["omfp:named", "omfp:named"]);
    expect(screen.getAllByTestId("widget").map((element) => element.dataset.path)).toEqual(["0.0", "0.1.0"]);
    expect(screen.getAllByTestId("named").map((element) => element.textContent)).toEqual(["first", "plain"]);
    const cells = screen
      .getByTestId("view")
      .querySelectorAll<HTMLElement>(":scope > [data-layout=row] > .omfp-view__cell");
    expect([...cells].map((cell) => cell.style.flexGrow)).toEqual(["2", "1"]);
    expect(screen.queryAllByTestId("widget-toolbar").length).toBe(0);
    // Splitters are always there (UI-206); only the toolbars wait for edit mode.
    expect(screen.getAllByTestId("splitter").map((splitter) => splitter.dataset.path)).toEqual(["0"]);
  });

  // spec: UI-024, UI-092
  test("a failing or unknown widget shows its error in place; the others still render", () => {
    const quiet = console.error;
    console.error = () => {};
    try {
      show(["col", {}, "omfp:broken", "omfp:missing", "omfp:named"]);
    } finally {
      console.error = quiet;
    }
    const [broken, missing] = screen.getAllByTestId("widget");
    expect(within(broken ?? document.body).getByTestId("widget-error").textContent).toContain("boom");
    expect(within(missing ?? document.body).getByTestId("widget-error").textContent).toBe(
      "Unknown widget omfp:missing",
    );
    expect(screen.getByTestId("named").textContent).toBe("plain");
  });

  // spec: UI-024, UI-092
  test("tabs show one widget at a time and switch from the keyboard", async () => {
    show(["tabs", {}, ["omfp:named", { title: "One", label: "1" }], ["omfp:named", { label: "2" }]]);
    const tabs = screen.getAllByTestId("view-tab");
    expect(tabs.map((tab) => tab.textContent)).toEqual(["One", "omfp:named"]);
    expect(screen.getByTestId("named").textContent).toBe("1");
    const user = userEvent.setup();
    await user.click(tabs[0] ?? document.body);
    await user.keyboard("{ArrowRight}");
    expect(screen.getByTestId("named").textContent).toBe("2");
  });

  // spec: AGT-010, UI-080
  test("a built-in without a title prop is labelled with its own name", () => {
    show(["tabs", {}, "omfp:workspace", "omfp:changes", ["omfp:changes", { title: "Diffs" }]]);
    expect(screen.getAllByTestId("view-tab").map((tab) => tab.textContent)).toEqual(["Workspace", "Changes", "Diffs"]);
  });

  // spec: AGT-010
  test("a tabs container with one child shows no tab bar, only that child", () => {
    show(["tabs", { id: "main" }, ["omfp:named", { label: "chat" }]]);
    expect(screen.queryAllByTestId("view-tab").length).toBe(0);
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(screen.getByTestId("named").textContent).toBe("chat");
    const tabs = document.querySelector("[data-layout=tabs]");
    expect(tabs?.getAttribute("data-path")).toBe("0");
    expect(screen.getByTestId("widget").dataset.path).toBe("0.0");
  });
});

describe("edit mode", () => {
  const LAYOUT = [
    "row",
    { sizes: [3, 1] },
    ["col", {}, "omfp:named", ["omfp:named", { label: "x" }]],
    ["tabs", {}, "omfp:other", "omfp:named"],
  ];

  // spec: UI-103
  test("every widget gets a toolbar with its reference, Move and Hide, widgets in tabs too", () => {
    show(LAYOUT, []);
    const toolbars = screen.getAllByTestId("widget-toolbar");
    expect(toolbars.map((toolbar) => [toolbar.dataset.path, toolbar.dataset.widget])).toEqual([
      ["0.0.0", "omfp:named"],
      ["0.0.1", "omfp:named"],
      ["0.1.0", "omfp:other"],
    ]);
    const [first] = toolbars;
    expect(first?.getAttribute("role")).toBe("toolbar");
    expect(first?.textContent).toContain("omfp:named");
    expect(within(first ?? document.body).getByTestId("widget-move").textContent).toBe("Move");
    expect(within(first ?? document.body).getByTestId("widget-hide").textContent).toBe("Hide");
    // The other tab's widget gets its toolbar once shown.
    act(() => {
      (screen.getAllByTestId("view-tab")[1] ?? document.body).click();
    });
    expect(screen.getAllByTestId("widget-toolbar").map((toolbar) => toolbar.dataset.path)).toContain("0.1.1");
  });

  // spec: UI-103
  test("Move lists every other widget (before, after) and every container (end); picking one edits", async () => {
    const edits: LayoutEdit[] = [];
    show(LAYOUT, edits);
    const user = userEvent.setup();
    const toolbar = screen.getAllByTestId("widget-toolbar")[1] ?? document.body;
    await user.click(within(toolbar).getByTestId("widget-move"));
    const targets = screen.getAllByTestId("move-target");
    expect(targets.map((target) => `${target.dataset.place} ${target.dataset.path}`)).toEqual([
      "before 0.0.0",
      "after 0.0.0",
      "end 0.0",
      "before 0.1.0",
      "after 0.1.0",
      "before 0.1.1",
      "after 0.1.1",
      "end 0.1",
      "end 0",
    ]);
    await user.click(
      targets.find((target) => target.dataset.place === "end" && target.dataset.path === "0") ?? toolbar,
    );
    expect(edits).toEqual([{ kind: "move", from: "0.0.1", place: "end", to: "0" }]);
  });

  // spec: UI-103
  test("Hide edits; the view's last widget cannot be hidden", async () => {
    const edits: LayoutEdit[] = [];
    show(LAYOUT, edits);
    const user = userEvent.setup();
    await user.click(within(screen.getAllByTestId("widget-toolbar")[2] ?? document.body).getByTestId("widget-hide"));
    expect(edits).toEqual([{ kind: "hide", path: "0.1.0" }]);
    cleanup();
    show("omfp:named", edits);
    expect((screen.getByTestId("widget-hide") as HTMLButtonElement).disabled).toBe(true);
  });

  // spec: UI-092, UI-103, UI-285
  test("after Hide the focus goes to the next widget's toolbar, else the previous one", async () => {
    const edits: LayoutEdit[] = [];
    const onEdit = async (edit: LayoutEdit) => {
      edits.push(edit);
      return true;
    };
    const labelled = (...labels: string[]) => ["col", {}, ...labels.map((label) => ["omfp:named", { label }])];
    const frame = (layout: unknown) => (
      <>
        <button type="button" data-testid="layout-edit">
          Edit layout
        </button>
        <ViewRenderer view={only({ views: { v: { layout } } })} registry={registry} onEdit={onEdit} />
      </>
    );
    const { host, view } = renderWithHost(frame(labelled("a", "b", "c")));
    const rerender = (layout: unknown) => view.rerender(<OmfpProvider host={host}>{frame(layout)}</OmfpProvider>);
    const user = userEvent.setup();
    const hide = (index: number) =>
      user.click(within(screen.getAllByTestId("widget-toolbar")[index] ?? document.body).getByTestId("widget-hide"));
    /** The focused control's test id and the path of the toolbar holding it. */
    const focused = () => {
      const element = document.activeElement;
      const toolbar = element?.closest<HTMLElement>('[data-testid="widget-toolbar"]');
      return [element?.getAttribute("data-testid"), toolbar?.dataset.path];
    };
    // The middle widget: the next one, which takes its place.
    await hide(1);
    rerender(labelled("a", "c"));
    await act(async () => {});
    expect(focused()).toEqual(["widget-move", "0.1"]);
    // The last widget: the previous one.
    await hide(1);
    rerender(labelled("a"));
    await act(async () => {});
    expect(focused()).toEqual(["widget-move", "0.0"]);
    expect(edits).toEqual([
      { kind: "hide", path: "0.1" },
      { kind: "hide", path: "0.1" },
    ]);
  });

  // spec: UI-103
  test("splitters between cells of rows and columns resize with the arrow keys in 5 % steps", () => {
    const edits: LayoutEdit[] = [];
    show(LAYOUT, edits);
    const splitters = screen.getAllByTestId("splitter");
    // Document order: the column's splitter sits inside the row's first cell.
    expect(splitters.map((splitter) => [splitter.dataset.path, splitter.dataset.index])).toEqual([
      ["0.0", "1"],
      ["0", "1"],
    ]);
    const [col, row] = splitters;
    expect(row?.getAttribute("role")).toBe("separator");
    expect(row?.getAttribute("aria-orientation")).toBe("vertical");
    expect(row?.getAttribute("aria-valuenow")).toBe("75");
    expect(col?.getAttribute("aria-orientation")).toBe("horizontal");
    expect(col?.getAttribute("aria-valuenow")).toBe("50");
    fireEvent.keyDown(row ?? document.body, { key: "ArrowRight" });
    fireEvent.keyDown(row ?? document.body, { key: "ArrowLeft" });
    fireEvent.keyDown(col ?? document.body, { key: "ArrowDown" });
    fireEvent.keyDown(col ?? document.body, { key: "Enter" });
    expect(edits).toEqual([
      { kind: "resize", path: "0", index: 1, delta: 0.05 },
      { kind: "resize", path: "0", index: 1, delta: -0.05 },
      { kind: "resize", path: "0.0", index: 1, delta: 0.05 },
    ]);
  });

  // spec: UI-103
  test("a pointer drag previews the new sizes and writes them once released", () => {
    const edits: LayoutEdit[] = [];
    show(LAYOUT, edits);
    const splitter = screen.getAllByTestId("splitter").find((element) => element.dataset.path === "0") ?? document.body;
    const container = splitter.parentElement ?? document.body;
    container.getBoundingClientRect = () => DOMRect.fromRect({ x: 0, y: 0, width: 400, height: 300 });
    splitter.setPointerCapture = () => {};
    const cells = () =>
      [...container.querySelectorAll<HTMLElement>(":scope > .omfp-view__cell")].map((cell) => cell.style.flexGrow);
    fireEvent.pointerDown(splitter, { pointerId: 1, clientX: 100, clientY: 0 });
    fireEvent.pointerMove(splitter, { pointerId: 1, clientX: 140, clientY: 0 });
    expect(cells()).toEqual(["3.4", "0.6"]);
    expect(edits).toEqual([]);
    fireEvent.pointerUp(splitter, { pointerId: 1, clientX: 140, clientY: 0 });
    expect(edits).toEqual([{ kind: "resize", path: "0", index: 1, delta: 0.1 }]);
    expect(cells()).toEqual(["3.4", "0.6"]);
  });
});
