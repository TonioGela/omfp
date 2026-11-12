/** L4: `omfp:background` over a fake client (UI-271..UI-273), fed the recorded async `bash` and `task` fixtures. */

import { afterEach, describe, expect, test } from "bun:test";
import type { ThreadEvent } from "@omfp/core";
import { cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { OPEN_SUBAGENT, REVEAL_WIDGET, SELECTED_THREAD } from "../../src/sdk/index.ts";
import { BackgroundWidget } from "../../src/widgets/background/index.ts";
import { renderWithHost, testHost } from "../support/host.tsx";
import { fixtureEvents, isFrame, summary } from "./chat/fixtures.ts";

afterEach(cleanup);

const THREAD = "01T";
const bash = fixtureEvents("async_bash_tool_call");
/** Up to the call's end: the job runs. */
const running = bash.slice(0, bash.findIndex(isFrame("tool_execution_end")) + 1);
const task = fixtureEvents("task_tool_call");

function setup(threadEvents: readonly ThreadEvent[]) {
  const base = testHost();
  base.client.answers.set("thread.subagents", () => ({ subagents: [] }));
  base.client.answers.set("thread.subagent_cancel", () => ({}));
  base.client.answers.set("thread.prompt", () => ({}));
  base.host.window.set(SELECTED_THREAD, THREAD);
  const test = renderWithHost(<BackgroundWidget />, base);
  test.client.connected();
  test.client.index({ threads: [summary(THREAD)] });
  test.client.thread(THREAD, threadEvents);
  test.flush();
  return { ...test, user: userEvent.setup() };
}

function rows() {
  return screen.getAllByTestId("background-row").map((row) => [row.dataset.kind, row.dataset.id, row.dataset.status]);
}

describe("omfp:background", () => {
  // spec: UI-271
  test("a running job: its command, state and last line", () => {
    setup(running);
    expect(rows()).toEqual([["job", "bg_1", "running"]]);
    expect(screen.getByTestId("background-label").textContent).toBe("sleep 1; echo bg-output-42");
    expect(screen.getByTestId("background-state").textContent).toBe("running");
    // The call's own result is omp's notice to the model: no output yet.
    expect(screen.queryByTestId("background-last-line")).toBeNull();
  });

  // spec: UI-271
  test("a running job's output: its last line", () => {
    // Up to the job's completed frame: its last running frame has output.
    setup(bash.slice(0, bash.findLastIndex(isFrame("tool_execution_update"))));
    expect(screen.getByTestId("background-last-line").textContent).toBe("bg-output-42");
  });

  // spec: UI-271
  test("a finished job keeps its final output; nothing to stop", () => {
    setup(bash);
    expect(rows()).toEqual([["job", "bg_1", "completed"]]);
    expect(screen.getByTestId("background-state").textContent).toBe("done");
    expect(screen.queryByTestId("background-stop")).toBeNull();
  });

  // spec: UI-272
  test("stopping a job asks the agent with a steer and shows the request", async () => {
    const test = setup(running);
    await test.user.click(screen.getByTestId("background-stop"));
    expect(test.client.callsTo("thread.prompt").map((call) => call.params)).toEqual([
      { threadId: THREAD, text: "Stop background job bg_1.", behavior: "steer" },
    ]);
    expect(screen.getByTestId("background-state").textContent).toBe("stop requested");
    expect(screen.queryByTestId("background-stop")).toBeNull();
  });

  // spec: UI-271, UI-272
  test("subagents: a running one is cancelled, a click opens it in omfp:subagents", async () => {
    const started = task.slice(0, task.findIndex(isFrame("subagent_progress")) + 1);
    const test = setup(started);
    await waitFor(() => expect(rows()).toEqual([["subagent", "Counter", "running"]]));
    expect(screen.getByTestId("background-label").textContent).toBe("Count the functions in src/lib.rs");
    await test.user.click(screen.getByTestId("background-cancel"));
    expect(test.client.callsTo("thread.subagent_cancel").map((call) => call.params)).toEqual([
      { threadId: THREAD, subagentId: "Counter" },
    ]);
    await test.user.click(screen.getByRole("button", { name: "Open subagent Counter" }));
    expect(test.host.window.get(OPEN_SUBAGENT)?.subagentId).toBe("Counter");
    expect(test.host.window.get(REVEAL_WIDGET)?.reference).toBe("omfp:subagents");
  });

  // spec: UI-271
  test("a job's row reveals the chat when its card is not shown", async () => {
    const test = setup(running);
    await test.user.click(screen.getByRole("button", { name: "Show the card of background job bg_1" }));
    expect(test.host.window.get(REVEAL_WIDGET)?.reference).toBe("omfp:chat");
  });

  // spec: UI-271
  test("nothing in the background", () => {
    setup(fixtureEvents("text_reply"));
    expect(screen.getByTestId("background-none").textContent).toContain("Nothing runs in the background");
  });
});
