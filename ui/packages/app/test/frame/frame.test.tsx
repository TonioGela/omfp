/** L4: the frame (banner, views, keymap, `:` line, theme, live region, notifications, dialogs) over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import type { CallMethod } from "@omfp/client";
import type { Settings, ThreadSummary, ViewsState } from "@omfp/protocol";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Frame, type FrameProps } from "../../src/frame/frame.tsx";
import { LAYOUT_EDIT } from "../../src/frame/layout.tsx";
import type { NotificationPort, ThreadNotification } from "../../src/frame/notifications.ts";
import { ACTIVE_VIEW, DRAFT_THREAD, ERRORS, SELECTED_THREAD, type WidgetComponent } from "../../src/sdk/index.ts";
import type { WidgetRegistry } from "../../src/view/resolve.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { type FakeClient, renderWithHost, testHost } from "../support/host.tsx";

afterEach(() => {
  cleanup();
  document.head.innerHTML = "";
  delete document.documentElement.dataset.colorScheme;
  delete document.documentElement.dataset.density;
  document.documentElement.removeAttribute("style");
});

const stub =
  (name: string): WidgetComponent =>
  () => <p data-testid={`stub-${name}`}>{name}</p>;

const registry: WidgetRegistry = new Map<string, WidgetComponent>([
  ["omfp:header", stub("header")],
  ["omfp:chat", stub("chat")],
  ["omfp:workspace", stub("workspace")],
  ["omfp:transcript", stub("transcript")],
]);

const threads: ThreadSummary[] = [thread("t1", "checkout", "w1"), thread("t2", "flaky", "w1")];

function setup(props: FrameProps = {}, connect = true) {
  // Module widgets (workspace overrides) stay loading: these tests are about the frame.
  const base = testHost(undefined, { importer: () => new Promise<unknown>(() => {}) });
  for (const method of ["threads.archive", "threads.delete", "settings.set"] as const satisfies readonly CallMethod[]) {
    base.client.answers.set(method, () => ({}));
  }
  let clock = 0;
  const test = renderWithHost(<Frame registry={registry} now={() => clock} {...props} />, base);
  if (connect) {
    test.client.connected();
    test.client.index({ workspaces: [workspace("w1"), workspace("w2")], threads });
  }
  test.flush();
  return {
    ...test,
    user: userEvent.setup(),
    tick: (ms: number) => {
      clock += ms;
    },
  };
}

function press(key: string, init: KeyboardEventInit = {}): void {
  act(() => {
    fireEvent.keyDown(document.activeElement ?? document.body, { key, ...init });
  });
}

function shownWidgets(): (string | undefined)[] {
  return screen.getAllByTestId("widget").map((widget) => widget.dataset.widget);
}

/** Selects `t1` (workspace `w1`) and answers its views topic with `state`. */
function inWorkspace(state: Partial<ViewsState> = {}, props: FrameProps = {}) {
  const test = setup(props);
  act(() => test.host.window.set(SELECTED_THREAD, "t1"));
  test.client.views("w1", state);
  test.flush();
  return test;
}

const MANIFEST = {
  default: "work",
  views: {
    work: { title: "Work", layout: ["row", {}, "omfp:chat", "omfp:workspace"] },
    review: { title: "Review", layout: ["col", {}, "omfp:transcript", "omfp:chat"] },
    lean: { title: "Lean", layout: "omfp:header" },
  },
};

describe("frame", () => {
  // spec: UI-024, UI-100
  test("the left column is the frame's; the default view renders its widgets; there is a view switcher", () => {
    setup();
    const left = screen.getByTestId("left-column");
    expect(within(left).getByTestId("sidebar")).toBeDefined();
    expect(left.contains(screen.getByTestId("view"))).toBe(false);
    expect(screen.getByTestId("view").dataset.view).toBe("default");
    expect(shownWidgets()).toEqual(["omfp:header", "omfp:chat", "omfp:workspace"]);
    expect(within(screen.getByTestId("view-switcher")).getByRole("button").textContent).toContain("Default");
    expect(screen.queryByTestId("safety-net")).toBeNull();
  });

  // spec: UI-003
  test("the banner shows while connecting and goes away once connected, without blocking the view", () => {
    const { client, flush } = setup({}, false);
    expect(screen.getByTestId("connection-banner").dataset.tone).toBe("info");
    expect(screen.getByTestId("view")).toBeDefined();
    client.connected();
    flush();
    expect(screen.queryByTestId("connection-banner")).toBeNull();
    act(() => client.model.setConnection({ kind: "reconnecting", attempt: 2, retryInMs: 1500, error: "closed" }));
    client.notify();
    flush();
    expect(screen.getByTestId("connection-banner").textContent).toBe(
      "Connection lost. Reconnecting in 2 s (attempt 2)…",
    );
  });

  // spec: UI-095, UI-091, UI-094, UI-097
  test("`:` opens the command line; `:theme dark` is window-local, `:density normal` goes to the daemon", async () => {
    const { client, flush, user } = setup();
    expect(document.documentElement.dataset.colorScheme).toBe("system");
    expect(document.documentElement.dataset.density).toBe("compact");
    press(":");
    const input = screen.getByTestId("command-line-input");
    expect(document.activeElement).toBe(input);
    await user.type(input, "the");
    expect(screen.getAllByTestId("command-option").map((option) => option.dataset.value)).toEqual(["theme"]);
    await user.keyboard("{Tab}");
    expect((input as HTMLInputElement).value).toBe("theme ");
    expect(screen.getAllByTestId("command-option").map((option) => option.dataset.value)).toEqual([
      "theme system",
      "theme light",
      "theme dark",
    ]);
    await user.type(input, "d{Enter}");
    expect(screen.queryByTestId("command-line")).toBeNull();
    expect(document.documentElement.dataset.colorScheme).toBe("dark");
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "density normal{Enter}");
    expect(client.callsTo("settings.set").map((call) => call.params)).toEqual([{ ui: { density: "normal" } }]);
    expect(document.documentElement.dataset.density).toBe("compact");
    client.indexEvents({ kind: "settings_changed", settings: { ui: { density: "normal" }, keymap: [], error: null } });
    flush();
    expect(document.documentElement.dataset.density).toBe("normal");
  });

  // spec: UI-095, UI-026
  test("`:archive` runs on the selected thread and Escape closes the line, giving the focus back", async () => {
    const { client, host, user } = setup();
    act(() => host.window.set(SELECTED_THREAD, "t1"));
    const row = screen.getAllByTestId("thread-row")[0];
    row?.focus();
    press(":");
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("command-line")).toBeNull();
    expect(document.activeElement).toBe(row ?? null);
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "arch");
    await user.click(screen.getAllByTestId("command-option")[0] ?? document.body);
    expect(client.callsTo("threads.archive").map((call) => call.params)).toEqual([{ threadId: "t1" }]);
    expect(host.window.get(SELECTED_THREAD)).toBeUndefined();
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "nope{Enter}");
    expect(screen.getByTestId("command-line-error").textContent).toBe("Unknown command: nope");
  });

  // spec: UI-095, UI-023, UI-021
  test("vim keys move through threads, `Space a` reaches the waiting thread, `Space n` opens a draft, `Space w` the + workspace field", () => {
    const { client, flush, host, tick } = setup();
    document.body.focus();
    press("j");
    expect(host.window.get(SELECTED_THREAD)).toBe("t1");
    press("j");
    expect(host.window.get(SELECTED_THREAD)).toBe("t2");
    press("g");
    press("g");
    expect(host.window.get(SELECTED_THREAD)).toBe("t1");
    press("G", { shiftKey: true });
    expect(host.window.get(SELECTED_THREAD)).toBe("t2");
    client.indexEvents({ kind: "thread_upserted", thread: thread("t1", "checkout", "w1", { needsAttention: true }) });
    flush();
    press(" ");
    tick(5000);
    press("a");
    expect(host.window.get(SELECTED_THREAD)).toBe("t2");
    press(" ");
    press("a");
    expect(host.window.get(SELECTED_THREAD)).toBe("t1");
    press(" ");
    press("n");
    expect(host.window.get(DRAFT_THREAD)).toEqual({ workspaceId: "w1" });
    expect(host.window.get(SELECTED_THREAD)).toBeUndefined();
    expect(screen.getByTestId("draft-thread")).toBeDefined();
    press(" ");
    press("w");
    expect(document.activeElement).toBe(screen.getByTestId("new-workspace-input"));
  });

  // spec: UI-023
  test("`:new` without a current workspace focuses the + workspace field", async () => {
    const { host, user } = setup();
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "new{Enter}");
    expect(host.window.get(DRAFT_THREAD)).toBeUndefined();
    expect(document.activeElement).toBe(screen.getByTestId("new-workspace-input"));
  });

  // spec: UI-095, UI-025
  test("`:rescan` completes like other commands and rescans the current workspace", async () => {
    const { client, host, user } = setup();
    client.answers.set("workspaces.rescan", () => workspace("w1"));
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "resc");
    // Without a current workspace there is nothing to rescan.
    expect(screen.queryAllByTestId("command-option").map((option) => option.dataset.value)).toEqual([]);
    await user.keyboard("{Escape}");
    act(() => host.window.set(SELECTED_THREAD, "t1"));
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "resc");
    expect(screen.getAllByTestId("command-option").map((option) => option.dataset.value)).toEqual(["rescan"]);
    await user.click(screen.getAllByTestId("command-option")[0] ?? document.body);
    expect(client.callsTo("workspaces.rescan").map((call) => call.params)).toEqual([{ workspaceId: "w1" }]);
  });

  // spec: UI-026, UI-092
  test("`:delete` asks in a dialog; confirming deletes the thread and clears the selection", async () => {
    const { client, host, user } = setup();
    act(() => host.window.set(SELECTED_THREAD, "t2"));
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "delete{Enter}");
    const dialog = screen.getByTestId("delete-thread-dialog");
    expect(dialog.getAttribute("role")).toBe("alertdialog");
    expect(within(dialog).getByRole("heading").textContent).toBe("Delete \u201cflaky\u201d?");
    await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId("cancel-delete-thread")));
    await user.click(screen.getByTestId("confirm-delete-thread"));
    expect(client.callsTo("threads.delete").map((call) => call.params)).toEqual([{ threadId: "t2" }]);
    expect(host.window.get(SELECTED_THREAD)).toBeUndefined();
  });

  // spec: UI-091
  test("the daemon's theme replaces the bundle's link once the index is live, and follows theme_changed", () => {
    const link = document.createElement("link");
    link.setAttribute("data-omfp-theme", "");
    document.head.append(link);
    const { client, flush } = setup({}, false);
    expect(document.head.querySelector("link[data-omfp-theme]")).toBe(link);
    client.connected();
    client.index({ theme: { css: ":root { --omfp-primary: red; }" } });
    flush();
    expect(document.head.querySelector("link[data-omfp-theme]")).toBeNull();
    expect(document.head.querySelector("style[data-omfp-theme]")?.textContent).toBe(":root { --omfp-primary: red; }");
    client.indexEvents({ kind: "theme_changed", theme: { css: null } });
    flush();
    expect(document.head.querySelector("style[data-omfp-theme]")?.textContent).toBe("");
  });

  // spec: UI-092
  test("the live region announces the selected thread's state and the threads awaiting input", () => {
    const { client, flush, host } = setup();
    act(() => host.window.set(SELECTED_THREAD, "t1"));
    expect(screen.getByTestId("live-region").textContent).toBe("checkout: idle");
    expect(screen.getByTestId("live-region").getAttribute("aria-live")).toBe("polite");
    client.indexEvents({ kind: "thread_upserted", thread: thread("t2", "flaky", "w1", { needsAttention: true }) });
    flush();
    expect(screen.getByTestId("live-region").textContent).toBe("checkout: idle. 1 thread awaits input");
  });

  // spec: UI-096
  test("notifications only without focus; permission on a gesture; a click focuses the window on the thread", () => {
    const shown: Array<{ notification: ThreadNotification; click: () => void }> = [];
    let permission: NotificationPermission = "default";
    let requested = 0;
    let focused = true;
    let windowFocused = 0;
    const port: NotificationPort = {
      permission: () => permission,
      requestPermission: async () => {
        requested += 1;
        permission = "granted";
        return permission;
      },
      show: (notification, click) => shown.push({ notification, click }),
    };
    const { client, flush, host } = setup({
      notifications: port,
      hasFocus: () => focused,
      focusWindow: () => {
        windowFocused += 1;
      },
    });
    fireEvent.pointerDown(document.body);
    fireEvent.keyDown(document.body, { key: "x" });
    expect(requested).toBe(1);
    client.indexEvents({ kind: "thread_upserted", thread: thread("t1", "checkout", "w1", { needsAttention: true }) });
    flush();
    expect(shown).toEqual([]);
    focused = false;
    client.indexEvents(
      { kind: "thread_upserted", thread: thread("t2", "flaky", "w1", { process: { state: "failed", message: "x" } }) },
      { kind: "thread_upserted", thread: thread("t1", "checkout", "w1", { settled: false }) },
    );
    flush();
    client.indexEvents({ kind: "thread_upserted", thread: thread("t1", "checkout", "w1") });
    flush();
    expect(shown.map(({ notification }) => [notification.threadId, notification.kind, notification.title])).toEqual([
      ["t2", "failed", "flaky"],
      ["t1", "finished", "checkout"],
    ]);
    act(() => shown[0]?.click());
    expect(windowFocused).toBe(1);
    expect(host.window.get(SELECTED_THREAD)).toBe("t2");
  });

  // spec: UI-092
  test("errors of failed actions show until dismissed", async () => {
    const { host, user } = setup();
    act(() => host.window.set(ERRORS, ["boom"]));
    expect(screen.getAllByTestId("error").map((error) => error.textContent)).toEqual(["boom×"]);
    await user.click(screen.getByTestId("dismiss-error"));
    expect(screen.queryByTestId("errors")).toBeNull();
  });
});

describe("daemon settings", () => {
  function settings(ui: Settings["ui"], keymap: Settings["keymap"] = [], error: string | null = null): Settings {
    return { ui, keymap, error };
  }

  // spec: UI-097, UI-094
  test("fonts become inline font tokens on <html>, removed again when unset", () => {
    const { client, flush } = setup();
    client.indexEvents({
      kind: "settings_changed",
      settings: settings({ fonts: { prose: "Iowan Old Style, serif", code: "Iosevka" } }),
    });
    flush();
    const style = document.documentElement.style;
    expect(style.getPropertyValue("--omfp-font-prose")).toBe("Iowan Old Style, serif");
    expect(style.getPropertyValue("--omfp-font-code")).toBe("Iosevka");
    expect(style.getPropertyValue("--omfp-font-ui")).toBe("");
    client.indexEvents({ kind: "settings_changed", settings: settings({}) });
    flush();
    expect(style.getPropertyValue("--omfp-font-prose")).toBe("");
  });

  // spec: UI-097, UI-094
  test("the font size becomes the root font-size on <html>, 16px without one", () => {
    const { client, flush } = setup();
    const style = document.documentElement.style;
    expect(style.fontSize).toBe("16px");
    client.indexEvents({ kind: "settings_changed", settings: settings({ fontSize: 20 }) });
    flush();
    expect(style.fontSize).toBe("20px");
    client.indexEvents({ kind: "settings_changed", settings: settings({}) });
    flush();
    expect(style.fontSize).toBe("16px");
  });

  // spec: UI-097, UI-095
  test("keymap entries apply live over the defaults", () => {
    const { client, flush, host } = setup();
    document.body.focus();
    client.indexEvents({
      kind: "settings_changed",
      settings: settings({}, [
        { mode: "normal", keys: "j", command: "" },
        { mode: "normal", keys: "n", command: "thread.new" },
      ]),
    });
    flush();
    press("j");
    expect(host.window.get(SELECTED_THREAD)).toBeUndefined();
    press("k");
    expect(host.window.get(SELECTED_THREAD)).toBe("t2");
    press("n");
    expect(host.window.get(DRAFT_THREAD)).toEqual({ workspaceId: "w1" });
  });

  // spec: UI-097
  test("`:set` changes a setting through settings.set; a bad line is reported", async () => {
    const { client, host, user } = setup();
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "set fonts.ui=Inter, sans-serif{Enter}");
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "set font_size=18{Enter}");
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "set keymap.insert.Ctrl-s=composer.send{Enter}");
    expect(client.callsTo("settings.set").map((call) => call.params)).toEqual([
      { ui: { fonts: { ui: "Inter, sans-serif" } } },
      { ui: { fontSize: 18 } },
      { keymap: [{ mode: "insert", keys: "Ctrl-s", command: "composer.send" }] },
    ]);
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "set density=huge{Enter}");
    expect(host.window.get(ERRORS)).toEqual(["Invalid value for density: huge"]);
  });

  // spec: UI-097
  test("a config file error shows a banner until the file is valid again", () => {
    const { client, flush } = setup();
    client.indexEvents({ kind: "settings_changed", settings: settings({}, [], "line 3: unknown key `colour`") });
    flush();
    expect(screen.getByTestId("settings-error").textContent).toContain("line 3: unknown key `colour`");
    client.indexEvents({ kind: "settings_changed", settings: settings({}) });
    flush();
    expect(screen.queryByTestId("settings-error")).toBeNull();
  });

  // spec: UI-097, UI-096
  test("notifications `always` show even with focus, `never` shows none and asks no permission", () => {
    const shown: string[] = [];
    let requested = 0;
    const port: NotificationPort = {
      permission: () => "granted",
      requestPermission: async () => {
        requested += 1;
        return "granted";
      },
      show: (notification) => shown.push(notification.threadId),
    };
    const { client, flush } = setup({ notifications: port, hasFocus: () => true });
    client.indexEvents({ kind: "settings_changed", settings: settings({ notifications: "always" }) });
    flush();
    client.indexEvents({ kind: "thread_upserted", thread: thread("t1", "checkout", "w1", { needsAttention: true }) });
    flush();
    expect(shown).toEqual(["t1"]);
    client.indexEvents({ kind: "settings_changed", settings: settings({ notifications: "never" }) });
    flush();
    client.indexEvents({ kind: "thread_upserted", thread: thread("t2", "flaky", "w1", { needsAttention: true }) });
    flush();
    expect([shown, requested]).toEqual([["t1"], 0]);
  });
});

describe("views of the current workspace", () => {
  // spec: UI-100
  test("the frame follows the current workspace's views topic; the default view until it answers", () => {
    const { client, flush, host } = setup();
    expect(client.subscribed.filter((topic) => topic.startsWith("views/"))).toEqual([]);
    act(() => host.window.set(SELECTED_THREAD, "t1"));
    expect(client.subscribed).toContain("views/w1");
    expect(screen.getByTestId("view").dataset.view).toBe("default");
    client.views("w1", { manifest: MANIFEST });
    flush();
    expect(screen.getByTestId("view").dataset.view).toBe("work");
    expect(shownWidgets()).toEqual(["omfp:chat", "omfp:workspace"]);
    // The draft thread's workspace is current once no thread is selected.
    act(() => {
      host.window.set(SELECTED_THREAD, undefined);
      host.window.set(DRAFT_THREAD, { workspaceId: "w2" });
    });
    expect(client.unsubscribed).toContain("views/w1");
    expect(client.subscribed).toContain("views/w2");
    expect(screen.getByTestId("view").dataset.view).toBe("default");
    client.views("w2");
    flush();
    expect(screen.getByTestId("view").dataset.view).toBe("default");
  });

  // spec: UI-101
  test("the switcher lists views by title in id order; the choice is window-local; `:view <id>` does the same", async () => {
    const { client, flush, host, user } = inWorkspace({ manifest: MANIFEST });
    const switcher = screen.getByTestId("view-switcher");
    expect(within(switcher).getByRole("button").textContent).toContain("Work");
    await user.click(within(switcher).getByRole("button"));
    const options = screen.getAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual(["Lean", "Review", "Work"]);
    await user.click(options[1] ?? document.body);
    expect(host.window.get(ACTIVE_VIEW)).toBe("review");
    expect(screen.getByTestId("view").dataset.view).toBe("review");
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "view lean{Enter}");
    expect(screen.getByTestId("view").dataset.view).toBe("lean");
    // The chosen view goes away: the manifest's default shows.
    client.viewsEvents("w1", {
      kind: "views_manifest",
      manifest: { ...MANIFEST, views: { work: MANIFEST.views.work, review: MANIFEST.views.review } },
      error: null,
    });
    flush();
    expect(screen.getByTestId("view").dataset.view).toBe("work");
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "view gone{Enter}");
    expect(host.window.get(ERRORS)).toEqual(["No view gone"]);
  });

  // spec: UI-102, AGT-011
  test("an invalid manifest shows the daemon's error above the default view, until a valid one arrives", () => {
    const { client, flush } = inWorkspace({ manifest: null, error: "views.broken.layout: row has no children" });
    expect(screen.getByTestId("view-error").textContent).toBe(
      "The view is invalid, showing the default view: views.broken.layout: row has no children",
    );
    expect(screen.getByTestId("view").dataset.view).toBe("default");
    client.viewsEvents("w1", { kind: "views_manifest", manifest: MANIFEST, error: null });
    flush();
    expect(screen.queryByTestId("view-error")).toBeNull();
    expect(screen.getByTestId("view").dataset.view).toBe("work");
  });

  // spec: AGT-015, AGT-022
  test("a view without built-in chat or interactions gets the safety net; a workspace override does not count", () => {
    const { client, flush, host } = inWorkspace({ manifest: MANIFEST });
    expect(screen.queryByTestId("safety-net")).toBeNull();
    act(() => host.window.set(ACTIVE_VIEW, "lean"));
    expect(screen.getByTestId("safety-net")).toBeDefined();
    act(() => host.window.set(ACTIVE_VIEW, "work"));
    expect(screen.queryByTestId("safety-net")).toBeNull();
    client.viewsEvents("w1", {
      kind: "views_widgets",
      widgets: [{ name: "chat", source: "workspace", version: "v1", entry: "index.tsx" }],
    });
    flush();
    expect(screen.getByTestId("safety-net")).toBeDefined();
  });
});

describe("layout editing", () => {
  /** The manifests written so far. */
  function written({ client }: { client: FakeClient }): unknown[] {
    return client.callsTo("views.set").map((call) => call.params);
  }

  /** In `w1` with `state`, in edit mode; `views.set` succeeds unless `answer` is false. */
  function editing(state: Partial<ViewsState> = {}, answer = true) {
    const test = inWorkspace(state);
    if (answer) {
      test.client.answers.set("views.set", () => ({}));
    }
    act(() => test.host.window.set(LAYOUT_EDIT, true));
    return test;
  }

  // spec: UI-103
  test("`Edit layout` toggles this window's edit mode (also `:layout`), only with a current workspace", async () => {
    const { host, user } = setup();
    const toggle = screen.getByTestId("layout-edit");
    expect((toggle as HTMLButtonElement).disabled).toBe(true);
    act(() => host.window.set(SELECTED_THREAD, "t1"));
    expect((toggle as HTMLButtonElement).disabled).toBe(false);
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(screen.queryAllByTestId("widget-toolbar")).toEqual([]);
    await user.click(toggle);
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    expect(host.window.get(LAYOUT_EDIT)).toBe(true);
    expect(screen.getAllByTestId("widget-toolbar").map((toolbar) => toolbar.dataset.widget)).toEqual([
      "omfp:header",
      "omfp:chat",
      "omfp:workspace",
    ]);
    expect(screen.getAllByTestId("splitter").length).toBe(2);
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "layout{Enter}");
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(screen.queryAllByTestId("widget-toolbar")).toEqual([]);
  });

  // spec: UI-103
  test("the first change without views.json writes the default view with it; the view follows the pushed manifest", async () => {
    const test = editing();
    const { client, flush, user } = test;
    const workspaceToolbar = screen
      .getAllByTestId("widget-toolbar")
      .find((toolbar) => toolbar.dataset.path === "0.1.0");
    await user.click(within(workspaceToolbar ?? document.body).getByTestId("widget-move"));
    await user.click(
      screen
        .getAllByTestId("move-target")
        .find((target) => target.dataset.place === "before" && target.dataset.path === "0.0.0") ?? document.body,
    );
    const manifest = {
      $schema: "omfp://schemas/views.json",
      default: "default",
      views: {
        default: {
          title: "Default",
          layout: [
            "row",
            { sizes: [3, 1] },
            ["col", {}, "omfp:workspace", "omfp:header", "omfp:chat"],
            ["tabs", {}, "omfp:changes"],
          ],
        },
      },
    };
    expect(written(test)).toEqual([{ workspaceId: "w1", manifest }]);
    // Rendering follows the daemon's push, not the local edit.
    expect(shownWidgets()).toEqual(["omfp:header", "omfp:chat", "omfp:workspace"]);
    client.viewsEvents("w1", { kind: "views_manifest", manifest, error: null });
    flush();
    expect(shownWidgets()).toEqual(["omfp:workspace", "omfp:header", "omfp:chat", "omfp:changes"]);
    await user.click(within(screen.getAllByTestId("widget-toolbar")[0] ?? document.body).getByTestId("widget-hide"));
    expect(written(test)[1]).toEqual({
      workspaceId: "w1",
      manifest: {
        ...manifest,
        views: {
          default: {
            title: "Default",
            layout: ["row", { sizes: [3, 1] }, ["col", {}, "omfp:header", "omfp:chat"], ["tabs", {}, "omfp:changes"]],
          },
        },
      },
    });
  });

  // spec: UI-103
  test("a splitter writes the resized view of the manifest", () => {
    const test = editing({ manifest: MANIFEST });
    fireEvent.keyDown(screen.getByTestId("splitter"), { key: "ArrowLeft" });
    expect(written(test)).toEqual([
      {
        workspaceId: "w1",
        manifest: {
          ...MANIFEST,
          views: {
            ...MANIFEST.views,
            work: { title: "Work", layout: ["row", { sizes: [0.9, 1.1] }, "omfp:chat", "omfp:workspace"] },
          },
        },
      },
    ]);
  });

  // spec: UI-103
  test("failed or refused writes land in the error list; an invalid views.json is never overwritten", async () => {
    const failing = editing({ manifest: MANIFEST }, false);
    await failing.user.click(
      within(screen.getAllByTestId("widget-toolbar")[0] ?? document.body).getByTestId("widget-hide"),
    );
    const [pending] = failing.client.callsTo("views.set");
    await act(async () => pending?.reject(new Error("views.json: permission denied")));
    expect(failing.host.window.get(ERRORS)).toEqual(["views.json: permission denied"]);
    cleanup();
    const invalid = editing({ manifest: null, error: "views: no view" });
    await invalid.user.click(
      within(screen.getAllByTestId("widget-toolbar")[0] ?? document.body).getByTestId("widget-hide"),
    );
    expect(written(invalid)).toEqual([]);
    expect(invalid.host.window.get(ERRORS)).toEqual([
      "The layout cannot change while .omfp/views.json is invalid: fix the file first",
    ]);
  });

  // spec: UI-104
  test("`Add widget` lists built-ins, workspace and user widgets; picking one adds it at the end and writes", async () => {
    const test = editing({
      manifest: MANIFEST,
      widgets: [
        { name: "tests", source: "workspace", version: "a", entry: "index.tsx" },
        { name: "clock", source: "user", version: "b", entry: "index.ts" },
      ],
    });
    const { host, user } = test;
    act(() => host.window.set(LAYOUT_EDIT, false));
    expect(screen.queryByTestId("widget-add")).toBeNull();
    act(() => host.window.set(LAYOUT_EDIT, true));
    await user.click(screen.getByTestId("widget-add"));
    const picker = screen.getByTestId("widget-picker");
    expect(
      within(picker)
        .getAllByTestId("widget-option")
        .map((option) => option.dataset.widget),
    ).toEqual(["omfp:header", "omfp:chat", "omfp:workspace", "omfp:transcript", "./widgets/tests", "user:clock"]);
    await user.click(
      within(picker)
        .getAllByTestId("widget-option")
        .find((option) => option.dataset.widget === "user:clock") ?? document.body,
    );
    expect(written(test)).toEqual([
      {
        workspaceId: "w1",
        manifest: {
          ...MANIFEST,
          views: {
            ...MANIFEST.views,
            work: { title: "Work", layout: ["row", {}, "omfp:chat", "omfp:workspace", "user:clock"] },
          },
        },
      },
    ]);
    await waitFor(() => expect(screen.queryByTestId("widget-picker")).toBeNull());
  });
});

describe("widget copy", () => {
  // spec: UI-106
  test("`:widget copy <reference> [<name>]` copies into the current workspace and announces ./widgets/<name>", async () => {
    const { client, host, user } = setup();
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "widget copy omfp:workspace{Enter}");
    expect(host.window.get(ERRORS)).toEqual(["No current workspace"]);
    act(() => host.window.set(ERRORS, []));
    act(() => host.window.set(SELECTED_THREAD, "t1"));
    client.answers.set("widgets.copy", () => ({ name: "board" }));
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "widget copy omfp:workspace board{Enter}");
    expect(client.callsTo("widgets.copy").map((call) => call.params)).toEqual([
      { workspaceId: "w1", widget: "omfp:workspace", name: "board" },
    ]);
    await waitFor(() => expect(screen.getByTestId("live-region").textContent).toBe("Copied to ./widgets/board"));
    client.answers.delete("widgets.copy");
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "widget copy user:clock{Enter}");
    const [, failing] = client.callsTo("widgets.copy");
    expect(failing?.params).toEqual({ workspaceId: "w1", widget: "user:clock" });
    await act(async () => failing?.reject(new Error("./widgets/clock already exists")));
    expect(host.window.get(ERRORS)).toEqual(["./widgets/clock already exists"]);
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "widget paste{Enter}");
    expect(host.window.get(ERRORS)).toEqual([
      "./widgets/clock already exists",
      "Usage: :widget copy <reference> [<name>]",
    ]);
  });
});
