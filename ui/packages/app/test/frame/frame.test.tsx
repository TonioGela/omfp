/** L4: the frame (banner, views, keymap, `:` line, theme, live region, notifications, dialogs) over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import type { CallMethod } from "@omfp/client";
import type { Settings, ThreadSummary } from "@omfp/protocol";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Frame, type FrameProps } from "../../src/frame/frame.tsx";
import type { NotificationPort, ThreadNotification } from "../../src/frame/notifications.ts";
import { DRAFT_THREAD, ERRORS, SELECTED_THREAD, type WidgetComponent } from "../../src/sdk/index.ts";
import type { WidgetRegistry } from "../../src/view/render.tsx";
import { ThreadsWidget } from "../../src/widgets/threads/index.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { renderWithHost, testHost } from "../support/host.tsx";

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
  ["omfp:threads", ThreadsWidget],
  ["omfp:header", stub("header")],
  ["omfp:chat", stub("chat")],
  ["omfp:workspace", stub("workspace")],
]);

const threads: ThreadSummary[] = [thread("t1", "checkout", "w1"), thread("t2", "flaky", "w1")];

function setup(props: FrameProps = {}, connect = true) {
  const base = testHost();
  for (const method of ["threads.archive", "threads.delete", "settings.set"] as const satisfies readonly CallMethod[]) {
    base.client.answers.set(method, () => ({}));
  }
  let clock = 0;
  const test = renderWithHost(<Frame registry={registry} now={() => clock} {...props} />, base);
  if (connect) {
    test.client.connected();
    test.client.index({ workspaces: [workspace("w1")], threads });
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

describe("frame", () => {
  // spec: UI-024
  test("the default view renders its widgets; there is a view switcher", () => {
    setup();
    expect(screen.getByTestId("view").dataset.view).toBe("default");
    expect(screen.getAllByTestId("widget").map((widget) => widget.dataset.widget)).toEqual([
      "omfp:threads",
      "omfp:header",
      "omfp:chat",
      "omfp:workspace",
    ]);
    expect(within(screen.getByTestId("view-switcher")).getByRole("button").textContent).toContain("Default");
    expect(screen.queryByTestId("safety-net")).toBeNull();
  });

  // spec: UI-024
  test("an invalid view manifest shows the default view with the error", () => {
    setup({ views: { views: { broken: { layout: ["row", {}] } } } });
    expect(screen.getByTestId("view-error").textContent).toContain("views.broken.layout: row has no children");
    expect(screen.getByTestId("view").dataset.view).toBe("default");
  });

  // spec: UI-024
  test("a view without chat or interactions gets the frame's safety net", () => {
    setup({ views: { views: { lean: { layout: "omfp:header" } } } });
    expect(screen.getByTestId("safety-net")).toBeDefined();
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
    expect(document.activeElement).toBe(screen.getByTestId("new-workspace-name"));
  });

  // spec: UI-023
  test("`:new` without a current workspace focuses the + workspace field", async () => {
    const { host, user } = setup();
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "new{Enter}");
    expect(host.window.get(DRAFT_THREAD)).toBeUndefined();
    expect(document.activeElement).toBe(screen.getByTestId("new-workspace-name"));
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
