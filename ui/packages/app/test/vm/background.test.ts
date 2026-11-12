import { describe, expect, test } from "bun:test";
import type { ToolRun } from "@omfp/core";
import {
  backgroundRows,
  type JobTimes,
  lastOutputLine,
  observeJobs,
  runningInBackground,
  stopJobMessage,
} from "../../src/vm/background.ts";
import type { SubagentRow } from "../../src/vm/subagents.ts";
import { fixture, fixtureUntil, isFrame } from "../widgets/chat/fixtures.ts";

const NO_TIMES: JobTimes = new Map();

function subagent(id: string, status: string, durationMs?: number): SubagentRow {
  return {
    id,
    index: 0,
    agent: "task",
    status,
    description: undefined,
    task: `task of ${id}`,
    activity: "Reading",
    meta: "",
    durationMs,
    live: status === "running",
  };
}

describe("background work", () => {
  // spec: UI-271
  test("the last non-blank line of a job's output", () => {
    expect(lastOutputLine("a\n b \n\n")).toBe("b");
    expect(lastOutputLine("")).toBe("");
  });

  // spec: UI-271
  test("rows: jobs newest first with command, state and last line, then subagents", () => {
    const tools = fixture("async_bash_tool_call").tools;
    const rows = backgroundRows(tools, [subagent("Counter", "aborted", 1500)], new Set(), NO_TIMES, 0);
    expect(rows).toEqual([
      {
        kind: "job",
        id: "bg_1",
        toolCallId: "call_bash_bg",
        label: "sleep 1; echo bg-output-42",
        state: "completed",
        durationMs: undefined,
        lastLine: rows[0]?.lastLine,
        stopRequested: false,
      },
      {
        kind: "subagent",
        id: "Counter",
        toolCallId: undefined,
        label: "task of Counter",
        state: "cancelled",
        durationMs: 1500,
        lastLine: "Reading",
        stopRequested: false,
      },
    ]);
    expect(rows[0]?.lastLine).toContain("Wall time");
    // Plain calls and calls of other tools are no jobs.
    expect(backgroundRows(fixture("bash_tool_call").tools, [], new Set(), NO_TIMES, 0)).toEqual([]);
  });

  // spec: UI-271, UI-272
  test("durations from when the window saw a job start and end; a stop request while it runs", () => {
    const running = fixtureUntil("async_bash_tool_call", isFrame("tool_execution_update")).tools;
    const seen = observeJobs(NO_TIMES, running, 1000);
    expect(observeJobs(seen, running, 1500)).toBe(seen);
    const [row] = backgroundRows(running, [], new Set(["bg_1"]), seen, 4000);
    expect([row?.state, row?.durationMs, row?.stopRequested]).toEqual(["running", 3000, true]);
    const done = fixture("async_bash_tool_call").tools;
    const ended = observeJobs(seen, done, 6000);
    const [finished] = backgroundRows(done, [], new Set(["bg_1"]), ended, 9000);
    expect([finished?.state, finished?.durationMs, finished?.stopRequested]).toEqual(["completed", 5000, false]);
  });

  // spec: UI-274
  test("what runs: running jobs and subagents", () => {
    const running = fixtureUntil("async_bash_tool_call", isFrame("tool_execution_update")).tools;
    const none: ToolRun[] = [];
    expect(runningInBackground(running, [subagent("A", "running"), subagent("B", "completed")])).toBe(2);
    expect(runningInBackground(fixture("async_bash_tool_call").tools, [])).toBe(0);
    expect(runningInBackground(none, [subagent("C", "progressing")])).toBe(1);
  });

  // spec: UI-272
  test("the steer that stops a job", () => {
    expect(stopJobMessage("bg_3")).toBe("Stop background job bg_3.");
  });
});
