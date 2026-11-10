/** L1/L4: view layouts, their validation and the renderer with its error boundaries (UI-024). */

import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { WidgetComponent } from "../../src/sdk/index.ts";
import { containsWidget, parseLayout } from "../../src/view/layout.ts";
import { ViewRenderer, type WidgetRegistry } from "../../src/view/render.tsx";
import { DEFAULT_VIEW, resolveViews } from "../../src/view/views.ts";

afterEach(cleanup);

const Named: WidgetComponent = (props) => <span data-testid="named">{String(props.label ?? "plain")}</span>;
const Broken: WidgetComponent = () => {
  throw new Error("boom");
};

const registry: WidgetRegistry = new Map([
  ["test:named", Named],
  ["test:broken", Broken],
]);

function widgets(): string[] {
  return screen.getAllByTestId("widget").map((element) => element.dataset.widget ?? "");
}

describe("layouts", () => {
  // spec: UI-024
  test("the default view is threads, header over chat, and the workspace", () => {
    const { views, defaultId, error } = resolveViews(undefined);
    expect([defaultId, error, views.length]).toEqual(["default", undefined, 1]);
    expect(DEFAULT_VIEW.layout).toEqual([
      "row",
      { sizes: [1, 3, 1] },
      "omfp:threads",
      ["col", {}, "omfp:header", "omfp:chat"],
      "omfp:workspace",
    ]);
    const tree = views[0]?.tree;
    expect(tree !== undefined && containsWidget(tree, ["omfp:chat"])).toBe(true);
    expect(tree !== undefined && containsWidget(tree, ["omfp:interactions"])).toBe(false);
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

describe("renderer", () => {
  // spec: UI-024
  test("containers render their widgets in order with their props, sizes as flex weights", () => {
    const { views } = resolveViews({
      views: {
        v: { layout: ["row", { sizes: [2, 1] }, ["test:named", { label: "first" }], ["col", {}, "test:named"]] },
      },
    });
    const view = views[0];
    if (view === undefined) {
      throw new Error("fixture");
    }
    render(<ViewRenderer view={view} registry={registry} />);
    expect(screen.getByTestId("view").dataset.view).toBe("v");
    expect(widgets()).toEqual(["test:named", "test:named"]);
    expect(screen.getAllByTestId("named").map((element) => element.textContent)).toEqual(["first", "plain"]);
    const cells = screen.getByTestId("view").querySelectorAll<HTMLElement>(":scope > [data-layout=row] > div");
    expect([...cells].map((cell) => cell.style.flexGrow)).toEqual(["2", "1"]);
  });

  // spec: UI-024, UI-092
  test("a failing or unknown widget shows its error in place; the others still render", () => {
    const { views } = resolveViews({
      views: { v: { layout: ["col", {}, "test:broken", "test:missing", "test:named"] } },
    });
    const view = views[0];
    if (view === undefined) {
      throw new Error("fixture");
    }
    const quiet = console.error;
    console.error = () => {};
    try {
      render(<ViewRenderer view={view} registry={registry} />);
    } finally {
      console.error = quiet;
    }
    const [broken, missing] = screen.getAllByTestId("widget");
    expect(within(broken ?? document.body).getByTestId("widget-error").textContent).toContain("boom");
    expect(within(missing ?? document.body).getByTestId("widget-error").textContent).toBe(
      "Unknown widget test:missing",
    );
    expect(screen.getByTestId("named").textContent).toBe("plain");
  });

  // spec: UI-024, UI-092
  test("tabs show one widget at a time and switch from the keyboard", async () => {
    const { views } = resolveViews({
      views: {
        v: { layout: ["tabs", {}, ["test:named", { title: "One", label: "1" }], ["test:named", { label: "2" }]] },
      },
    });
    const view = views[0];
    if (view === undefined) {
      throw new Error("fixture");
    }
    render(<ViewRenderer view={view} registry={registry} />);
    const tabs = screen.getAllByTestId("view-tab");
    expect(tabs.map((tab) => tab.textContent)).toEqual(["One", "test:named"]);
    expect(screen.getByTestId("named").textContent).toBe("1");
    const user = userEvent.setup();
    await user.click(tabs[0] ?? document.body);
    await user.keyboard("{ArrowRight}");
    expect(screen.getByTestId("named").textContent).toBe("2");
  });
});
