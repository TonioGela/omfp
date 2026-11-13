/** L4: `thread.stopSpawned` (`:stop!`, `Space S`), the cascade stop of the selected thread (UI-242). */

import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, waitFor } from "@testing-library/react";
import { GlobalCommands } from "../../src/frame/commands.tsx";
import { DEFAULT_BINDINGS } from "../../src/keymap/keymap.ts";
import { NOTICES, SELECTED_THREAD, SUCCESSES } from "../../src/sdk/index.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { ManualTimers, renderWithHost, testHost } from "../support/host.tsx";

afterEach(cleanup);

function setup(selected: string) {
  const base = testHost(undefined, { noticeTimers: new ManualTimers() });
  base.host.window.set(SELECTED_THREAD, selected);
  base.client.answers.set("thread.stop", () => ({ threadIds: ["t1", "t2", "t3"] }));
  const test = renderWithHost(
    <GlobalCommands viewIds={[]} currentView={undefined} widgets={[]} addWidget={() => Promise.resolve(false)} />,
    base,
  );
  test.client.connected();
  test.client.index({
    workspaces: [workspace("w1")],
    threads: [
      thread("t1", "orchestrator", "w1"),
      thread("t2", "fe", "w1", { provenance: { createdBy: { kind: "thread", threadId: "t1" } } }),
      thread("t3", "be", "w1", { provenance: { createdBy: { kind: "thread", threadId: "t2" } } }),
    ],
  });
  test.flush();
  return test;
}

describe("thread.stopSpawned", () => {
  // spec: UI-242, PRT-085, UI-520
  test("stops the selected thread with its spawned tree and says how many stopped in a success toast", async () => {
    const { client, host } = setup("t1");
    act(() => {
      expect(host.commands.run("thread.stopSpawned")).toBe(true);
    });
    expect(client.callsTo("thread.stop").map((call) => call.params)).toEqual([{ threadId: "t1", cascade: true }]);
    await waitFor(() => expect(host.window.get(SUCCESSES).length).toBe(1));
    expect(host.window.get(SUCCESSES).map((success) => success.text)).toEqual(["orchestrator: Stopped 3 threads"]);
    expect(host.announcer.getSnapshot().message).toBe("orchestrator: Stopped 3 threads");
    // A user command's outcome, not a thread event: no mode-line notice.
    expect(host.window.get(NOTICES)).toEqual([]);
  });

  // spec: UI-242
  test("is offered only for a thread that spawned threads, on `Space S` and `:stop!`", () => {
    const { host } = setup("t3");
    expect(host.commands.resolve("thread.stopSpawned")).toBeUndefined();
    expect(
      DEFAULT_BINDINGS.some((binding) => binding.keys === "Space S" && binding.command === "thread.stopSpawned"),
    ).toBe(true);
    act(() => host.window.set(SELECTED_THREAD, "t2"));
    expect(host.commands.resolve("stop!")?.id).toBe("thread.stopSpawned");
  });
});
