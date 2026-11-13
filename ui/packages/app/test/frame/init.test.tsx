/** L4: init files and `:describe-key` / `:describe-command` in the window (UI-260..UI-266, experimental). */

import { afterEach, describe, expect, test } from "bun:test";
import type { CallMethod } from "@omfp/client";
import type { ThreadSummary } from "@omfp/protocol";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import { Frame } from "../../src/frame/frame.tsx";
import { initUrl } from "../../src/frame/init.tsx";
import { type InitApi, SELECTED_THREAD, type WidgetComponent } from "../../src/sdk/index.ts";
import type { WidgetRegistry } from "../../src/view/resolve.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { renderWithHost, type TestHost, testHost } from "../support/host.tsx";

afterEach(() => {
  cleanup();
  document.head.innerHTML = "";
});

const stub =
  (name: string): WidgetComponent =>
  () => <p>{name}</p>;

const registry: WidgetRegistry = new Map<string, WidgetComponent>([
  ["omfp:header", stub("header")],
  ["omfp:chat", stub("chat")],
  ["omfp:workspace", stub("workspace")],
  ["omfp:changes", stub("changes")],
]);

const USER_URL = initUrl("/widgets/test", "user", "u1");
const WORKSPACE_URL = initUrl("/widgets/test", "w1", "v1");

let log: string[] = [];

const userInit = (omfp: InitApi) => {
  omfp.command({
    id: "my.hello",
    title: "Say hello",
    description: "Greets from the user's init file.",
    aliases: ["hello"],
    run: (args) => {
      log.push(`hello ${args.join(" ")}`);
    },
  });
  omfp.bind({ keys: "g h", command: "my.hello", args: ["user"] });
  omfp.on("attention", (summary: ThreadSummary) => {
    log.push(`attention ${summary.id}`);
  });
};

const workspaceInit = (omfp: InitApi) => {
  // The workspace's file wins over the user's for the same keys.
  omfp.bind({ keys: "g h", command: "my.hello", args: ["workspace"] });
  omfp.views.transform((views) => views.add("solo", { title: "Solo", layout: "omfp:chat" }));
};

function setup() {
  log = [];
  const modules: Record<string, unknown> = {
    [USER_URL]: { default: userInit },
    [WORKSPACE_URL]: { default: workspaceInit },
    [initUrl("/widgets/test", "user", "u2")]: {
      default: () => {
        throw new Error("boom");
      },
    },
  };
  const base = testHost(undefined, {
    importer: (url) => (url in modules ? Promise.resolve(modules[url]) : new Promise<unknown>(() => {})),
  });
  for (const method of ["widgets.report"] as const satisfies readonly CallMethod[]) {
    base.client.answers.set(method, () => ({}));
  }
  const test = renderWithHost(<Frame registry={registry} now={() => 0} />, base);
  test.client.connected();
  test.client.index({ workspaces: [workspace("w1")], threads: [thread("t1", "checkout", "w1")], init: "u1" });
  act(() => test.host.window.set(SELECTED_THREAD, "t1"));
  test.client.views("w1", { init: "v1" });
  test.flush();
  return test;
}

/** Waits for the loads queued so far. */
async function settled(test: TestHost): Promise<void> {
  await act(async () => {
    await test.host.init.settled();
  });
  test.flush();
}

function keys(...sequence: string[]): void {
  for (const key of sequence) {
    act(() => {
      fireEvent.keyDown(document.activeElement ?? document.body, { key });
    });
  }
}

/** The fields of the describe dialog's sections: `[section, label, text]`. */
function fields(): string[][] {
  return screen.queryAllByTestId("describe-section").flatMap((section) =>
    within(section)
      .queryAllByTestId("describe-field")
      .map((field) => [
        section.getAttribute("data-section") ?? "",
        field.querySelector("dt")?.textContent ?? "",
        field.querySelector("dd")?.textContent ?? "",
      ]),
  );
}

describe("init files in the window", () => {
  // spec: UI-260, UI-261, UI-262, UI-266
  test("the user's file, then the workspace's, add commands, bindings, views and thread listeners", async () => {
    const test = setup();
    await settled(test);
    keys("g", "h");
    expect(log).toEqual(["hello workspace"]);
    expect(
      within(screen.getByTestId("view-switcher"))
        .getAllByRole("option", { hidden: true })
        .map((option) => option.textContent),
    ).toContain("Solo");

    test.client.indexEvents({
      kind: "thread_upserted",
      thread: thread("t1", "checkout", "w1", { needsAttention: true }),
    });
    test.flush();
    expect(log).toEqual(["hello workspace", "attention t1"]);

    // Without the workspace's file, the user's binding is back.
    test.client.viewsEvents("w1", { kind: "views_init", init: null });
    test.flush();
    await settled(test);
    keys("g", "h");
    expect(log.at(-1)).toBe("hello user");
  });

  // spec: UI-263
  test("a failing file shows a banner, loses what it registered, and the window keeps working", async () => {
    const test = setup();
    await settled(test);
    expect(screen.queryAllByTestId("init-error")).toHaveLength(0);
    test.client.indexEvents({ kind: "init_changed", init: "u2" });
    test.flush();
    await settled(test);
    const banners = screen.getAllByTestId("init-error");
    expect(banners.map((banner) => [banner.getAttribute("data-file"), banner.textContent])).toEqual([
      ["user", "Init file the user's init.ts failed, the window works without it: boom"],
    ]);
    expect(test.host.commands.resolve("my.hello")).toBeUndefined();
    keys("?");
    expect(screen.getByTestId("help")).toBeDefined();
  });

  // spec: UI-264
  test(":describe-key takes keys and shows the command, its description and where it was bound", async () => {
    const test = setup();
    await settled(test);
    act(() => {
      test.host.commands.run("describe-key");
    });
    expect(screen.getByTestId("describe-prompt").textContent).toBe("Press the keys to describe (Escape cancels)");
    keys("g");
    expect(screen.getByTestId("describe-prompt").textContent).toBe("g …");
    keys("h");
    expect(log).toEqual([]);
    expect(fields()).toEqual([
      ["normal:g h", "keys", "g h"],
      ["normal:g h", "command", "my.hello"],
      ["normal:g h", "title", "Say hello"],
      ["normal:g h", "description", "Greets from the user's init file."],
      ["normal:g h", "defined by", "the user's init.ts"],
      ["normal:g h", "arguments", "workspace"],
      ["normal:g h", "bound in", "the workspace's .omfp/init.ts"],
    ]);

    act(() => {
      test.host.commands.run("describe-key");
    });
    keys("Space", "n");
    expect(fields()).toContainEqual(["normal:Space n", "bound in", "default keymap"]);
    keys("Escape");
  });

  // spec: UI-265
  test(":describe-command names a command's names, scope, origin and every key bound to it", async () => {
    const test = setup();
    await settled(test);
    act(() => {
      test.host.commands.run("describe-command", ["hello"]);
    });
    expect(fields()).toEqual([
      [":my.hello", "command", "my.hello"],
      [":my.hello", "title", "Say hello"],
      [":my.hello", "description", "Greets from the user's init file."],
      [":my.hello", "defined by", "the user's init.ts"],
      [":my.hello", "names", ":my.hello :hello"],
      [":my.hello", "scope", "global"],
      [":my.hello", "normal", "g h workspace · the workspace's .omfp/init.ts"],
    ]);
    act(() => {
      test.host.commands.run("describe-command", ["nothing.here"]);
    });
    expect(fields()).toEqual([["unknown", "command", "No command answers to nothing.here"]]);
  });
});
