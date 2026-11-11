/** L4: `omfp:subagents` over a fake client (UI-081), fed the recorded `task` fixture. */

import { afterEach, describe, expect, test } from "bun:test";
import type { ThreadEvent } from "@omfp/core";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SELECTED_THREAD } from "../../src/sdk/index.ts";
import { SubagentsWidget } from "../../src/widgets/subagents/index.ts";
import { renderWithHost, testHost } from "../support/host.tsx";
import { fixtureEvents, isFrame, summary } from "./chat/fixtures.ts";

afterEach(cleanup);

const THREAD = "01T";
const events = fixtureEvents("task_tool_call");

const TASK = { role: "user", content: [{ type: "text", text: "Count the functions in src/lib.rs" }], timestamp: 0 };
const CALL = {
  role: "assistant",
  content: [{ type: "toolCall", id: "c1", name: "read", arguments: {}, intent: "Reading the library" }],
};
const ANSWER = { role: "assistant", content: [{ type: "text", text: "src/lib.rs defines 2 functions." }] };

function setup(threadEvents: readonly ThreadEvent[], listed: unknown[] = []) {
  const base = testHost();
  base.client.answers.set("thread.subagents", () => ({ subagents: listed }));
  base.client.answers.set("thread.subagent_steer", () => ({}));
  base.client.answers.set("thread.subagent_cancel", () => ({}));
  base.host.window.set(SELECTED_THREAD, THREAD);
  const test = renderWithHost(<SubagentsWidget />, base);
  test.client.connected();
  test.client.index({ threads: [summary(THREAD)] });
  test.client.thread(THREAD, threadEvents);
  test.flush();
  return { ...test, user: userEvent.setup() };
}

describe("omfp:subagents", () => {
  // spec: UI-081, PRT-055
  test("the roster from the thread's subagent frames and the running ones omp lists", async () => {
    const test = setup(events, [{ id: "Scout", index: 1, agent: "scout", status: "running", task: "Look around" }]);
    expect(test.client.callsTo("thread.subagents").map((call) => call.params)).toEqual([{ threadId: THREAD }]);
    await waitFor(() => expect(screen.getAllByTestId("subagent-row").length).toBe(2));
    const [counter, scout] = screen.getAllByTestId("subagent-row");
    expect([
      counter?.dataset.subagent,
      counter?.dataset.status,
      scout?.dataset.subagent,
      scout?.dataset.status,
    ]).toEqual(["Counter", "completed", "Scout", "running"]);
    expect(counter?.textContent).toContain("Count the functions in src/lib.rs");
    expect(counter?.textContent).toContain("1 tool · fake/scripted");
  });

  // spec: UI-081, UI-187
  test("opening one reads its transcript, then only what is new from `nextByte`", async () => {
    const test = setup(events);
    await test.user.click(screen.getByRole("button", { name: "Open the transcript of Counter" }));
    const box = screen.getByTestId("subagent-transcript");
    expect(box.classList.contains("omfp-bbox")).toBe(true);
    const reads = () => test.client.callsTo("thread.subagent_messages");
    expect(reads().map((call) => call.params)).toEqual([{ threadId: THREAD, subagentId: "Counter", fromByte: 0 }]);
    reads()[0]?.resolve({
      sessionFile: "s",
      fromByte: 0,
      nextByte: 400,
      reset: false,
      entries: [],
      messages: [TASK, CALL],
    });
    await waitFor(() => expect(within(box).getAllByTestId("subagent-line").length).toBe(2));
    await test.user.click(within(box).getByTestId("subagent-more"));
    expect(reads()[1]?.params).toEqual({ threadId: THREAD, subagentId: "Counter", fromByte: 400 });
    reads()[1]?.resolve({
      sessionFile: "s",
      fromByte: 400,
      nextByte: 520,
      reset: false,
      entries: [],
      messages: [ANSWER],
    });
    await waitFor(() =>
      expect(
        within(box)
          .getAllByTestId("subagent-line")
          .map((line) => [line.dataset.role, line.textContent]),
      ).toEqual([
        ["user", "▸ taskCount the functions in src/lib.rs"],
        ["tool", "→ read: Reading the library"],
        ["assistant", "◆ subagentsrc/lib.rs defines 2 functions."],
      ]),
    );
    // A finished subagent cannot be steered.
    expect(within(box).queryByTestId("subagent-steer")).toBeNull();
  });

  // spec: UI-081
  test("a running subagent is steered and cancelled", async () => {
    const running = events.slice(0, events.findIndex(isFrame("subagent_progress")) + 1);
    const test = setup(running);
    await test.user.click(screen.getByRole("button", { name: "Open the transcript of Counter" }));
    await test.user.type(screen.getByTestId("subagent-steer-input"), "Only count public ones");
    await test.user.click(screen.getByTestId("subagent-steer"));
    expect(test.client.callsTo("thread.subagent_steer").map((call) => call.params)).toEqual([
      { threadId: THREAD, subagentId: "Counter", text: "Only count public ones" },
    ]);
    await test.user.click(screen.getByTestId("subagent-cancel"));
    expect(test.client.callsTo("thread.subagent_cancel").map((call) => call.params)).toEqual([
      { threadId: THREAD, subagentId: "Counter" },
    ]);
  });
});
