import { describe, expect, test } from "bun:test";
import type { ToolRun } from "@omfp/core";
import {
  type BackgroundRow,
  backgroundRows,
  backgroundSections,
  clearFinished,
  FINISHED_SHOWN,
  type JobTimes,
  lastOutputLine,
  NO_MEMORY,
  observeFinished,
  observeJobs,
  openRow,
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

function row(id: string, state: BackgroundRow["state"], kind: BackgroundRow["kind"] = "job"): BackgroundRow {
  return {
    kind,
    id,
    toolCallId: undefined,
    label: id,
    state,
    durationMs: undefined,
    lastLine: undefined,
    stopRequested: false,
    subagent: undefined,
  };
}

function ids(rows: readonly BackgroundRow[]): string[] {
  return rows.map((shown) => shown.id);
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
    const counter = subagent("Counter", "aborted", 1500);
    const rows = backgroundRows(tools, [counter], new Set(), NO_TIMES, 0);
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
        subagent: undefined,
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
        subagent: counter,
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

  // spec: UI-641
  test("sections: what runs, the most recently finished in sight, the older ones collapsed", () => {
    // Rows as backgroundRows gives them: jobs newest first.
    const rows = [row("bg_9", "running"), ...[8, 7, 6, 5, 4, 3, 2].map((n) => row(`bg_${n}`, "completed"))];
    rows.push(row("bg_1", "cancelled"));
    // Already finished when the window looked: their order; bg_9 finishes later and goes first.
    const seen = observeFinished(new Map(), rows, 100);
    expect(observeFinished(seen, rows, 200)).toBe(seen);
    const sections = backgroundSections(rows, seen, NO_MEMORY);
    expect([ids(sections.running), ids(sections.recent), ids(sections.older)]).toEqual([
      ["bg_9"],
      ["bg_8", "bg_7", "bg_6", "bg_5", "bg_4"],
      ["bg_3", "bg_2", "bg_1"],
    ]);
    expect(sections.recent.length).toBe(FINISHED_SHOWN);
    const later = [row("bg_9", "completed"), ...rows.slice(1)];
    const after = backgroundSections(later, observeFinished(seen, later, 300), NO_MEMORY);
    expect([ids(after.running), ids(after.recent).slice(0, 2), ids(after.older)]).toEqual([
      [],
      ["bg_9", "bg_8"],
      ["bg_4", "bg_3", "bg_2", "bg_1"],
    ]);
  });

  // spec: UI-642, UI-643
  test("Clear finished hides every finished row; a failed one stays in sight until opened or cleared", () => {
    const failed = row("Broken", "failed", "subagent");
    const rows = [row("bg_8", "running"), ...[7, 6, 5, 4, 3, 2, 1].map((n) => row(`bg_${n}`, "completed")), failed];
    const times = observeFinished(new Map(), rows, 0);
    const sections = backgroundSections(rows, times, NO_MEMORY);
    // The failed subagent finished first, yet stays in sight beside the five most recent.
    expect(ids(sections.recent)).toEqual(["bg_7", "bg_6", "bg_5", "bg_4", "bg_3", "Broken"]);
    expect(ids(sections.older)).toEqual(["bg_2", "bg_1"]);

    const opened = openRow(NO_MEMORY, failed);
    expect(opened).toEqual({ cleared: [], opened: ["subagent:Broken"] });
    expect(openRow(opened, failed)).toBe(opened);
    expect(openRow(NO_MEMORY, row("bg_7", "completed"))).toBe(NO_MEMORY);
    expect(ids(backgroundSections(rows, times, opened).older)).toEqual(["bg_2", "bg_1", "Broken"]);

    const cleared = clearFinished(opened, sections);
    expect(cleared.opened).toEqual([]);
    const after = backgroundSections(rows, times, cleared);
    expect([ids(after.running), ids(after.recent), ids(after.older)]).toEqual([["bg_8"], [], []]);
    // What finishes afterwards shows again.
    const more = [row("bg_8", "failed"), ...rows.slice(1)];
    const next = backgroundSections(more, observeFinished(times, more, 10), cleared);
    expect([ids(next.running), ids(next.recent)]).toEqual([[], ["bg_8"]]);
  });
});
