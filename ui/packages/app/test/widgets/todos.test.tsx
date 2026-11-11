/** L4: `omfp:todos` over a fake client (UI-082). */

import { afterEach, describe, expect, test } from "bun:test";
import type { ThreadEvent } from "@omfp/core";
import { cleanup, screen, within } from "@testing-library/react";
import { SELECTED_THREAD } from "../../src/sdk/index.ts";
import { TodosWidget } from "../../src/widgets/todos/index.ts";
import { renderWithHost, testHost } from "../support/host.tsx";
import { fixtureEvents, summary } from "./chat/fixtures.ts";

afterEach(cleanup);

const THREAD = "01T";

function setup(events: readonly ThreadEvent[], select = true) {
  const base = testHost();
  if (select) {
    base.host.window.set(SELECTED_THREAD, THREAD);
  }
  const test = renderWithHost(<TodosWidget />, base);
  test.client.connected();
  test.client.index({ threads: [summary(THREAD)] });
  test.client.thread(THREAD, events);
  test.flush();
  return test;
}

const goal = {
  id: "g1",
  objective: "Ship the tree widget",
  status: "active",
  tokensUsed: 1200,
  timeUsedSeconds: 4,
  createdAt: 0,
  updatedAt: 0,
};

describe("omfp:todos", () => {
  // spec: UI-082, UI-187, PRT-057
  test("each phase of the thread's todo list is a box titled on its border with its progress", () => {
    setup(fixtureEvents("todo_tool_calls"));
    const [phase] = screen.getAllByTestId("todos-phase");
    expect(phase?.classList.contains("omfp-bbox")).toBe(true);
    expect(within(phase as HTMLElement).getByRole("heading", { level: 3, name: "Tasks" })).toBeDefined();
    expect(phase?.querySelector(".omfp-bbox__end")?.textContent).toBe("2/2");
    expect(
      screen
        .getAllByTestId("todos-task")
        .map((task) => [task.querySelector(".omfp-todo__text")?.textContent, task.dataset.status]),
    ).toEqual([
      ["Read the code", "completed"],
      ["Fix the bug", "completed"],
    ]);
    expect(screen.queryByTestId("todos-goal")).toBeNull();
  });

  // spec: UI-082, PRT-056
  test("the session's goal above the list, following goal updates", () => {
    const test = setup(fixtureEvents("text_reply"));
    expect(screen.getByTestId("todos-none").textContent).toContain("No todo list");
    test.client.threadEvents(THREAD, { kind: "omp", frame: { type: "goal_updated", goal } } as ThreadEvent);
    test.flush();
    const box = screen.getByTestId("todos-goal");
    expect(box.textContent).toContain("Ship the tree widget");
    expect(box.textContent).toContain("1.2k tokens");
    expect(box.querySelector(".omfp-bbox__end")?.textContent).toBe("active");
    test.client.threadEvents(THREAD, { kind: "omp", frame: { type: "goal_updated", goal: null } } as ThreadEvent);
    test.flush();
    expect(screen.queryByTestId("todos-goal")).toBeNull();
  });

  // spec: UI-082
  test("without a selected thread, an empty state", () => {
    setup([], false);
    expect(screen.getByTestId("todos-no-thread").textContent).toContain("No thread selected");
  });
});
