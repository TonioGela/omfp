/** L4: the first run's welcome when omp has no usable model (UI-500..UI-504), over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import { CallError } from "@omfp/client";
import { act, cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Frame } from "../../src/frame/frame.tsx";
import { SELECTED_THREAD, type WidgetComponent } from "../../src/sdk/index.ts";
import type { WidgetRegistry } from "../../src/view/resolve.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { ManualTimers, renderWithHost, testHost } from "../support/host.tsx";

afterEach(() => {
  cleanup();
  document.head.innerHTML = "";
});

const stub =
  (name: string): WidgetComponent =>
  () => <p>{name}</p>;

const registry: WidgetRegistry = new Map<string, WidgetComponent>(
  ["header", "chat", "workspace", "changes", "background"].map((name) => [`omfp:${name}`, stub(name)]),
);

/** A connected window whose index knows `models` (`undefined`: the daemon has not checked yet). */
function setup(models: number | undefined) {
  const base = testHost(undefined, {
    importer: () => new Promise<unknown>(() => {}),
    reportTimers: new ManualTimers(),
    noticeTimers: new ManualTimers(),
  });
  base.client.answers.set("widgets.report", () => ({}));
  const test = renderWithHost(<Frame registry={registry} now={() => 0} />, base);
  test.client.connected();
  test.client.index({
    workspaces: [workspace("w1")],
    threads: [thread("t1", "checkout", "w1")],
    ...(models === undefined ? {} : { models }),
  });
  test.flush();
  return { ...test, user: userEvent.setup() };
}

/** Whether the welcome shows instead of the view. */
function welcomed(): boolean {
  const welcome = screen.queryAllByTestId("welcome").length === 1;
  expect(screen.queryAllByTestId("view").length).toBe(welcome ? 0 : 1);
  return welcome;
}

describe("the first run", () => {
  // spec: UI-500
  test("without a usable model and no thread selected, the main area welcomes and says why instead of the view", () => {
    setup(0);
    expect(welcomed()).toBe(true);
    const welcome = screen.getByTestId("welcome");
    expect(welcome.querySelector("h1")?.textContent).toBe("Welcome to omfp");
    expect(screen.getByTestId("welcome-why").textContent).toBe(
      "omp has no model it can use yet. Log in to a provider to start your first thread.",
    );
    expect(screen.getByTestId("welcome-keys").textContent).toContain("ANTHROPIC_API_KEY");
    // The rest of the frame stays: the threads column and the mode line.
    expect(screen.queryAllByTestId("left-column").length).toBe(1);
    expect(screen.queryAllByTestId("mode-bar").length).toBe(1);
  });

  // spec: UI-501
  test("its primary action opens the provider picker, and is busy while a login runs", async () => {
    const test = setup(0);
    test.client.answers.set("auth.providers", () => ({
      providers: [{ id: "anthropic", name: "Anthropic (Claude Pro/Max)", available: true, authenticated: false }],
    }));
    const button = screen.getByTestId("welcome-login");
    expect(button.textContent).toBe("Log in to a provider");
    expect(button.dataset.variant).toBe("primary");

    await test.user.click(button);

    await screen.findByTestId("login-picker");
    expect(test.client.callsTo("auth.providers").length).toBe(1);
    await test.user.click(await screen.findByTestId("login-provider"));
    await waitFor(() => expect(test.client.callsTo("auth.login").length).toBe(1));
    expect(screen.getByTestId("welcome-login").dataset.pending).toBe("true");
  });

  // spec: UI-502
  test("the window asks auth.status once per connection while the index does not know; a failure changes nothing", async () => {
    const test = setup(undefined);
    expect(test.client.callsTo("auth.status").length).toBe(1);
    expect(welcomed()).toBe(false);
    const [status] = test.client.callsTo("auth.status");
    await act(async () => {
      status?.reject(new CallError({ kind: "daemon", error: { code: "thread_failed", message: "omp is missing" } }));
      await Promise.resolve();
    });
    expect(welcomed()).toBe(false);
    expect(screen.queryAllByTestId("error").length).toBe(0);
    test.flush();
    expect(test.client.callsTo("auth.status").length).toBe(1);

    // A known count asks nothing.
    cleanup();
    const known = setup(3);
    expect(known.client.callsTo("auth.status").length).toBe(0);
  });

  // spec: UI-503
  test("the count follows the index live: a login that made models usable shows the view, losing them welcomes", () => {
    const test = setup(undefined);
    const models = (count: number) =>
      act(() => {
        test.client.indexEvents({ kind: "models_changed", models: count });
        test.flush();
      });
    models(0);
    expect(welcomed()).toBe(true);
    models(2);
    expect(welcomed()).toBe(false);
    models(0);
    expect(welcomed()).toBe(true);
  });

  // spec: UI-504
  test("with usable models, an unknown count or a selected thread, nothing changes", () => {
    const configured = setup(4);
    expect(welcomed()).toBe(false);
    expect(configured.client.callsTo("auth.status").length).toBe(0);
    cleanup();
    setup(undefined);
    expect(welcomed()).toBe(false);
    cleanup();
    const selected = setup(0);
    expect(welcomed()).toBe(true);
    act(() => selected.host.window.set(SELECTED_THREAD, "t1"));
    expect(welcomed()).toBe(false);
  });
});
