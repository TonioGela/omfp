/** L4: widgets a reveal opens as tabs of the view's `main` slot (UI-373), over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import type { ThreadEvent } from "@omfp/core";
import type { ViewsState } from "@omfp/protocol";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Frame } from "../../src/frame/frame.tsx";
import {
  type KeyValueStorage,
  type OmfpHost,
  OPEN_FILES,
  openFile,
  REVEAL_WIDGET,
  SELECTED_THREAD,
  useWidget,
  type WidgetComponent,
  widgetTabId,
} from "../../src/sdk/index.ts";
import type { WidgetRegistry } from "../../src/view/resolve.ts";
import { BackgroundWidget } from "../../src/widgets/background/index.ts";
import { SubagentsWidget } from "../../src/widgets/subagents/index.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { ManualTimers, renderWithHost, testHost } from "../support/host.tsx";
import { fixtureEvents, isFrame } from "../widgets/chat/fixtures.ts";

afterEach(() => {
  cleanup();
  document.head.innerHTML = "";
});

/** A widget showing the instance it renders as, so the tests see which `useStored` state it keeps. */
function ShownInstance({ name }: { name: string }) {
  return <p data-testid={`stub-${name}`}>{useWidget().instance}</p>;
}

const Instance =
  (name: string): WidgetComponent =>
  () => <ShownInstance name={name} />;

const STUBS: WidgetRegistry = new Map<string, WidgetComponent>([
  ["omfp:header", () => <p>header</p>],
  ["omfp:chat", () => <p data-testid="stub-chat">chat</p>],
  ["omfp:workspace", Instance("workspace")],
  ["omfp:changes", () => <p>changes</p>],
  ["omfp:background", Instance("background")],
  ["omfp:subagents", Instance("subagents")],
  ["omfp:todos", Instance("todos")],
  ["omfp:tree", Instance("tree")],
  ["omfp:file", () => <p data-testid="stub-file">file</p>],
]);

class MemoryStorage implements KeyValueStorage {
  readonly items = new Map<string, string>();
  getItem(key: string): string | null {
    return this.items.get(key) ?? null;
  }
  setItem(key: string, value: string): void {
    this.items.set(key, value);
  }
}

function setup(
  options: {
    views?: Partial<ViewsState>;
    storage?: MemoryStorage;
    registry?: WidgetRegistry;
    events?: readonly ThreadEvent[];
  } = {},
) {
  const storage = options.storage ?? new MemoryStorage();
  const base = testHost(undefined, {
    importer: () => new Promise<unknown>(() => {}),
    reportTimers: new ManualTimers(),
    storage,
  });
  base.client.answers.set("widgets.report", () => ({}));
  base.client.answers.set("thread.subagents", () => ({ subagents: [] }));
  const test = renderWithHost(<Frame registry={options.registry ?? STUBS} now={() => 0} />, base);
  test.client.connected();
  test.client.index({ workspaces: [workspace("w1")], threads: [thread("t1", "one", "w1")] });
  act(() => test.host.window.set(SELECTED_THREAD, "t1"));
  if (options.events !== undefined) {
    test.client.thread("t1", options.events);
  }
  if (options.views !== undefined) {
    test.client.views("w1", options.views);
  }
  test.flush();
  return { ...test, storage, user: userEvent.setup() };
}

function reveal(test: { host: OmfpHost; flush: () => void }, reference: string, nonce: number): void {
  act(() => test.host.window.set(REVEAL_WIDGET, { reference, nonce }));
  test.flush();
}

/** The tabs of the `main` slot. */
function tabElements(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>('[data-slot="main"] [role="tab"]')];
}

function tabs(): string[] {
  return tabElements().map((tab) => tab.textContent ?? "");
}

function selectedTab(): string | undefined {
  return tabElements().find((tab) => tab.getAttribute("aria-selected") === "true")?.textContent ?? undefined;
}

function widgetTab(reference: string): HTMLElement {
  return tabElements().find((tab) => tab.dataset.tab === widgetTabId(reference)) as HTMLElement;
}

describe("revealed widgets as tabs of the main slot", () => {
  // spec: UI-373, UI-274
  test("a widget the view lacks opens as a selected tab after the files; revealing it again selects it", async () => {
    const test = setup();
    act(() => openFile(test.host, "fe/src/lib.rs"));
    reveal(test, "omfp:subagents", 1);
    expect(tabs()).toEqual(["Chat", "lib.rs", "Subagents"]);
    expect(selectedTab()).toBe("Subagents");
    expect(widgetTab("omfp:subagents").dataset.testid).toBe("widget-tab");
    // Its instance is its reference: the same `useStored` state as a view node without `instance`.
    expect(within(screen.getByTestId("view")).getByTestId("stub-subagents").textContent).toBe("omfp:subagents");
    expect(screen.queryAllByTestId("floating-widget").length).toBe(0);
    // The request is done; the tab is window-local, kept in localStorage, never written to views.json.
    expect(test.host.window.get(REVEAL_WIDGET)).toBeUndefined();
    expect(JSON.parse(test.storage.getItem("omfp.openFiles") ?? "{}").w1).toMatchObject({
      widgets: ["omfp:subagents"],
      active: widgetTabId("omfp:subagents"),
    });
    expect(test.client.callsTo("views.set")).toEqual([]);

    // A file opened later goes with the files; revealing the widget again selects its one tab.
    act(() => openFile(test.host, "fe/README.md"));
    expect(tabs()).toEqual(["Chat", "lib.rs", "README.md", "Subagents"]);
    await test.user.click(tabElements()[0] as HTMLElement);
    expect(selectedTab()).toBe("Chat");
    reveal(test, "omfp:subagents", 2);
    expect(tabs()).toEqual(["Chat", "lib.rs", "README.md", "Subagents"]);
    expect(selectedTab()).toBe("Subagents");

    // Each reveal goes the same way: a widget the view holds (the side tabs' Workspace) is shown in place, no tab;
    // another it lacks opens its tab too.
    reveal(test, "omfp:workspace", 3);
    expect(tabs()).toEqual(["Chat", "lib.rs", "README.md", "Subagents"]);
    expect(test.host.window.get(REVEAL_WIDGET)).toBeUndefined();
    reveal(test, "omfp:todos", 4);
    expect(tabs()).toEqual(["Chat", "lib.rs", "README.md", "Subagents", "Todos"]);
    expect(selectedTab()).toBe("Todos");
  });

  // spec: UI-373, UI-365, UI-366
  test("a widget tab closes like a file tab: its ×, a middle click, Delete", async () => {
    const test = setup();
    act(() => openFile(test.host, "fe/src/lib.rs"));
    reveal(test, "omfp:subagents", 1);
    reveal(test, "omfp:tree", 2);
    reveal(test, "omfp:todos", 3);
    expect(tabs()).toEqual(["Chat", "lib.rs", "Subagents", "Tree", "Todos"]);

    // The selected one's own ×: its neighbour is selected.
    expect(screen.getAllByTestId("widget-tab-close").length).toBe(3);
    await test.user.click(within(widgetTab("omfp:todos")).getByTestId("widget-tab-close"));
    expect(tabs()).toEqual(["Chat", "lib.rs", "Subagents", "Tree"]);
    expect(selectedTab()).toBe("Tree");
    // A middle click closes the tab under the pointer, without the browser's autoscroll.
    expect(fireEvent.mouseDown(widgetTab("omfp:subagents"), { button: 1 })).toBe(false);
    fireEvent(widgetTab("omfp:subagents"), new MouseEvent("auxclick", { button: 1, bubbles: true, cancelable: true }));
    expect(tabs()).toEqual(["Chat", "lib.rs", "Tree"]);
    // Delete closes the focused one; the last file is its neighbour.
    act(() => widgetTab("omfp:tree").focus());
    await test.user.keyboard("{Delete}");
    expect(tabs()).toEqual(["Chat", "lib.rs"]);
    expect(selectedTab()).toBe("lib.rs");
    expect(test.host.window.get(OPEN_FILES).w1?.widgets).toEqual([]);
  });

  // spec: UI-373
  test("the open widget tabs and the selected one come back after a reload", () => {
    const storage = new MemoryStorage();
    const first = setup({ storage });
    reveal(first, "omfp:subagents", 1);
    expect(selectedTab()).toBe("Subagents");
    cleanup();
    setup({ storage });
    expect(tabs()).toEqual(["Chat", "Subagents"]);
    expect(selectedTab()).toBe("Subagents");
    expect(screen.getByTestId("stub-subagents").textContent).toBe("omfp:subagents");
  });

  // spec: UI-373, UI-274
  test("a view without a main slot floats the revealed widget as before", () => {
    const test = setup({
      views: { manifest: { default: "plain", views: { plain: { layout: ["row", {}, "omfp:chat", "omfp:changes"] } } } },
    });
    reveal(test, "omfp:subagents", 1);
    const panel = screen.getByTestId("floating-widget");
    expect(within(panel).getByTestId("floating-widget-title").textContent).toBe("Subagents");
    expect(within(panel).getByTestId("stub-subagents").textContent).toBe("omfp:subagents");
    expect(test.host.window.get(OPEN_FILES).w1).toBeUndefined();
    expect(tabs()).toEqual([]);
  });

  // spec: UI-373, UI-271
  test("a subagent opened from omfp:background shows its transcript in the Subagents tab", async () => {
    const task = fixtureEvents("task_tool_call");
    const started = task.slice(0, task.findIndex(isFrame("subagent_progress")) + 1);
    const registry: WidgetRegistry = new Map<string, WidgetComponent>([
      ...STUBS,
      ["omfp:background", BackgroundWidget],
      ["omfp:subagents", SubagentsWidget],
    ]);
    const test = setup({
      registry,
      events: started,
      views: {
        manifest: {
          default: "work",
          views: { work: { layout: ["row", {}, ["tabs", { id: "main" }, "omfp:chat"], "omfp:background"] } },
        },
      },
    });
    test.client.answers.set("thread.subagent_messages", () => ({
      sessionFile: "s",
      fromByte: 0,
      nextByte: 10,
      reset: false,
      entries: [],
      messages: [{ role: "user", content: [{ type: "text", text: "Count the functions" }], timestamp: 0 }],
    }));
    await test.user.click(await screen.findByRole("button", { name: "Open subagent Counter" }));
    test.flush();
    await waitFor(() => expect(selectedTab()).toBe("Subagents"));
    const main = document.querySelector<HTMLElement>('[data-slot="main"]') as HTMLElement;
    const box = await within(main).findByTestId("subagent-transcript");
    expect(test.client.callsTo("thread.subagent_messages").map((call) => call.params)).toEqual([
      { threadId: "t1", subagentId: "Counter", fromByte: 0 },
    ]);
    await waitFor(() =>
      expect(within(box).getAllByTestId("subagent-line")[0]?.textContent).toBe("▸ taskCount the functions"),
    );
    expect(screen.queryAllByTestId("floating-widget").length).toBe(0);
  });
});
