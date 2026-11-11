/** L4: opening files in the frame (UI-115, UI-116) and vim's mode on the mode line (UI-112), over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import type { ViewsState } from "@omfp/protocol";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Frame } from "../../src/frame/frame.tsx";
import {
  EDITOR_MODE,
  type KeyValueStorage,
  OPEN_FILES,
  openFile,
  SELECTED_THREAD,
  useCommand,
  type WidgetComponent,
} from "../../src/sdk/index.ts";
import type { WidgetRegistry } from "../../src/view/resolve.ts";
import { fileTabId } from "../../src/vm/files.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { ManualTimers, renderWithHost, testHost } from "../support/host.tsx";

afterEach(() => {
  cleanup();
  document.head.innerHTML = "";
});

/** `omfp:file` as its props, so the tests see what a tab asks for. */
const StubFile: WidgetComponent = (props) => <p data-testid="stub-file">{JSON.stringify(props)}</p>;

/** A text field `i` focuses, standing for the file editor. */
const StubChat: WidgetComponent = () => {
  useCommand({ id: "composer.focus", title: "Focus", run: () => document.getElementById("stub-input")?.focus() });
  return (
    <div>
      <textarea id="stub-input" data-testid="stub-input" />
      <button type="button" data-testid="card" data-omfp-path="fe/src/lib.rs" data-omfp-line="4">
        card
      </button>
      <button type="button" data-testid="changed" data-omfp-path="fe/src/main.rs" data-omfp-diff="">
        changed
      </button>
    </div>
  );
};

const registry: WidgetRegistry = new Map<string, WidgetComponent>([
  ["omfp:header", () => <p>header</p>],
  ["omfp:chat", StubChat],
  ["omfp:workspace", () => <p>workspace</p>],
  ["omfp:changes", () => <p>changes</p>],
  ["omfp:file", StubFile],
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

function setup(views?: Partial<ViewsState>) {
  const storage = new MemoryStorage();
  const base = testHost(undefined, {
    importer: () => new Promise<unknown>(() => {}),
    reportTimers: new ManualTimers(),
    storage,
  });
  base.client.answers.set("widgets.report", () => ({}));
  const test = renderWithHost(<Frame registry={registry} now={() => 0} />, base);
  test.client.connected();
  test.client.index({ workspaces: [workspace("w1")], threads: [thread("t1", "one", "w1")] });
  act(() => test.host.window.set(SELECTED_THREAD, "t1"));
  if (views !== undefined) {
    test.client.views("w1", views);
  }
  test.flush();
  return { ...test, storage, user: userEvent.setup() };
}

function press(key: string): void {
  act(() => {
    fireEvent.keyDown(document.activeElement ?? document.body, { key });
  });
}

/** The tabs of the `main` slot (the default view's side tabs are another container). */
function tabElements(): HTMLElement[] {
  return [...document.querySelectorAll<HTMLElement>('[data-slot="main"] [role="tab"]')];
}

function tabs(): string[] {
  return tabElements().map((tab) => tab.textContent ?? "");
}

describe("file tabs in the main slot", () => {
  // spec: UI-115, AGT-027
  test("a file opens as a selected tab after the chat; opening it again applies the line; closing goes back", async () => {
    const test = setup();
    // One tab needs no tab bar.
    expect(tabs()).toEqual([]);
    act(() => openFile(test.host, "fe/src/lib.rs", { line: 3 }));
    expect(tabs()).toEqual(["Chat", "lib.rs"]);
    const selected = tabElements().find((tab) => tab.getAttribute("aria-selected") === "true");
    expect(selected?.dataset.file).toBe("fe/src/lib.rs");
    expect(JSON.parse(screen.getByTestId("stub-file").textContent ?? "")).toEqual({
      path: "fe/src/lib.rs",
      line: 3,
      jump: 1,
    });
    act(() => openFile(test.host, "fe/README.md", { diff: true }));
    expect(tabs()).toEqual(["Chat", "lib.rs", "README.md"]);
    expect(JSON.parse(screen.getByTestId("stub-file").textContent ?? "")).toEqual({
      path: "fe/README.md",
      diff: true,
      jump: 1,
    });
    act(() => openFile(test.host, "fe/src/lib.rs", { line: 9 }));
    expect(JSON.parse(screen.getByTestId("stub-file").textContent ?? "")).toEqual({
      path: "fe/src/lib.rs",
      line: 9,
      jump: 2,
    });
    // Window-local, kept in localStorage, never written to views.json.
    expect(JSON.parse(test.storage.getItem("omfp.openFiles") ?? "{}").w1.active).toBe(fileTabId("fe/src/lib.rs"));
    expect(test.client.callsTo("views.set")).toEqual([]);
    await test.user.click(screen.getByTestId("file-tab-close"));
    expect(tabs()).toEqual(["Chat", "README.md"]);
    await test.user.click(screen.getByTestId("file-tab-close"));
    expect(tabs()).toEqual([]);
    expect(screen.getByTestId("stub-input")).toBeDefined();
  });

  // spec: UI-115
  test("selecting the chat tab keeps the files open; tabs are per workspace", async () => {
    const test = setup();
    act(() => openFile(test.host, "a.rs"));
    await test.user.click(tabElements()[0] as HTMLElement);
    expect(screen.getByTestId("stub-input")).toBeDefined();
    expect(test.host.window.get(OPEN_FILES).w1?.tabs.map((tab) => tab.path)).toEqual(["a.rs"]);
    expect(test.host.window.get(OPEN_FILES).w1?.active).toBe("0.0.1.0");
  });

  // spec: UI-115
  test("a view without a main slot opens the file in a floating panel, closed by its button or Esc", async () => {
    const test = setup({
      manifest: { default: "plain", views: { plain: { layout: ["row", {}, "omfp:chat", "omfp:workspace"] } } },
    });
    act(() => openFile(test.host, "fe/src/lib.rs"));
    const panel = screen.getByTestId("floating-file");
    expect(within(panel).getByTestId("stub-file")).toBeDefined();
    await test.user.click(within(panel).getByTestId("floating-file-close"));
    expect(screen.queryByTestId("floating-file")).toBeNull();
    act(() => openFile(test.host, "fe/src/main.rs"));
    act(() => screen.getByTestId("floating-file-close").focus());
    press("Escape");
    expect(screen.queryByTestId("floating-file")).toBeNull();
  });
});

describe("opening paths", () => {
  // spec: UI-116
  test("`g f` opens the path the focused element carries; `:open path:line` opens a typed one", async () => {
    const test = setup();
    act(() => screen.getByTestId("card").focus());
    press("g");
    press("f");
    expect(JSON.parse(screen.getByTestId("stub-file").textContent ?? "")).toEqual({
      path: "fe/src/lib.rs",
      line: 4,
      jump: 1,
    });
    await test.user.click(tabElements()[0] as HTMLElement);
    act(() => screen.getByTestId("changed").focus());
    press("g");
    press("f");
    expect(JSON.parse(screen.getByTestId("stub-file").textContent ?? "")).toEqual({
      path: "fe/src/main.rs",
      diff: true,
      jump: 1,
    });
    act(() => (document.activeElement as HTMLElement | null)?.blur());
    press(":");
    await test.user.keyboard("open docs/a.md:7{Enter}");
    expect(JSON.parse(screen.getByTestId("stub-file").textContent ?? "")).toEqual({
      path: "docs/a.md",
      line: 7,
      jump: 1,
    });
  });
});

describe("mode line", () => {
  // spec: UI-112
  test("while the file editor has the focus the mode line shows vim's mode", () => {
    const test = setup();
    const line = screen.getByTestId("mode-line");
    press("i");
    expect(line.textContent).toBe("INSERT");
    act(() => test.host.window.set(EDITOR_MODE, "visual"));
    expect([line.textContent, line.dataset.vim]).toEqual(["VISUAL", "visual"]);
    act(() => test.host.window.set(EDITOR_MODE, "normal"));
    expect(line.textContent).toBe("NORMAL");
    act(() => test.host.window.set(EDITOR_MODE, undefined));
    expect(line.textContent).toBe("INSERT");
  });
});
