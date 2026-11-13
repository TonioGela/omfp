/**
 * Background work of a thread (UI-271..UI-274): its omp background jobs — `bash` calls that became jobs (`details.async`,
 * kept by the reducer as `ToolRun.background`, PRT-100) — and its subagents (`vm/subagents.ts`), as the rows of the
 * `omfp:background` widget and the count of the header's badge.
 */

import type { JobState, ToolRun } from "@omfp/core";
import { isObject } from "../sdk/json.ts";
import type { SubagentRow } from "./subagents.ts";

/** One row of `omfp:background`. */
export interface BackgroundRow {
  readonly kind: "job" | "subagent";
  /** The job id (`bg_1`) or the subagent id. */
  readonly id: string;
  /** The call that started a job: its card in the transcript. */
  readonly toolCallId: string | undefined;
  /** A job's command; a subagent's description, else its task. */
  readonly label: string;
  readonly state: JobState;
  /** Run time so far, or the whole run once it ended; `undefined` when unknown. */
  readonly durationMs: number | undefined;
  /** A job's last output line; what a subagent does now. */
  readonly lastLine: string | undefined;
  /** Stop was asked for a job (a steer, UI-272) that still runs. */
  readonly stopRequested: boolean;
  /** A subagent's roster row, which its detail shows (UI-490); `undefined` for a job. */
  readonly subagent: SubagentRow | undefined;
}

/** The last non-blank line of `output`, trimmed; `""` without one. */
export function lastOutputLine(output: string): string {
  const lines = output.split("\n");
  for (let index = lines.length - 1; index >= 0; index--) {
    const line = (lines[index] ?? "").trim();
    if (line !== "") {
      return line;
    }
  }
  return "";
}

/** The `bash` runs of a thread that became background jobs, in call order. */
export function backgroundJobs(tools: readonly ToolRun[]): ToolRun[] {
  return tools.filter((run) => run.tool_name === "bash" && run.background !== null);
}

/** The job state of a subagent status: `aborted` is cancelled; a status omp may add later counts as running. */
function subagentState(status: string): JobState {
  switch (status) {
    case "completed":
      return "completed";
    case "failed":
      return "failed";
    case "aborted":
      return "cancelled";
    default:
      return "running";
  }
}

/** When this window first saw each job run and saw it end (job id → times), on its monotonic clock
 * (`performance.now()`, UI-681): omp's job frames carry no times. */
export type JobTimes = ReadonlyMap<string, { readonly start: number; readonly end: number | undefined }>;

/** `times` with the jobs of `tools` seen at `now`: new jobs start, ended ones get their end; unchanged otherwise. */
export function observeJobs(times: JobTimes, tools: readonly ToolRun[], now: number): JobTimes {
  let next: Map<string, { start: number; end: number | undefined }> | undefined;
  for (const run of backgroundJobs(tools)) {
    const job = run.background;
    if (job === null) {
      continue;
    }
    const seen = times.get(job.job_id);
    const ended = job.state !== "running";
    if (seen === undefined || (ended && seen.end === undefined)) {
      next ??= new Map(times);
      next.set(job.job_id, { start: seen?.start ?? now, end: ended ? now : undefined });
    }
  }
  return next ?? times;
}

/**
 * The rows of `omfp:background`: the jobs of `tools`, newest first, then the subagents; `stopping` holds the jobs
 * whose stop was asked for, `times` the job times this window saw, `now` the current time.
 */
export function backgroundRows(
  tools: readonly ToolRun[],
  subagents: readonly SubagentRow[],
  stopping: ReadonlySet<string>,
  times: JobTimes,
  now: number,
): BackgroundRow[] {
  const jobs = backgroundJobs(tools).flatMap((run): BackgroundRow[] => {
    const job = run.background;
    if (job === null) {
      return [];
    }
    const command = isObject(run.args) && typeof run.args.command === "string" ? run.args.command : undefined;
    const seen = times.get(job.job_id);
    const line = lastOutputLine(job.output ?? "");
    return [
      {
        kind: "job",
        id: job.job_id,
        toolCallId: run.tool_call_id,
        label: command ?? job.job_id,
        state: job.state,
        // Never below zero (UI-680), whatever reading `now` is.
        durationMs: seen === undefined ? undefined : Math.max(0, (seen.end ?? now) - seen.start),
        lastLine: line === "" ? undefined : line,
        stopRequested: job.state === "running" && stopping.has(job.job_id),
        subagent: undefined,
      },
    ];
  });
  const agents = subagents.map(
    (row): BackgroundRow => ({
      kind: "subagent",
      id: row.id,
      toolCallId: undefined,
      label: row.description ?? row.task,
      state: subagentState(row.status),
      durationMs: row.durationMs === undefined ? undefined : Math.max(0, row.durationMs),
      lastLine: row.activity,
      stopRequested: false,
      subagent: row,
    }),
  );
  return [...jobs.reverse(), ...agents];
}

/** How many jobs and subagents of a thread run (UI-274). */
export function runningInBackground(tools: readonly ToolRun[], subagents: readonly SubagentRow[]): number {
  return (
    backgroundJobs(tools).filter((run) => run.background?.state === "running").length +
    subagents.filter((row) => subagentState(row.status) === "running").length
  );
}

/** How many finished jobs and subagents `omfp:background` keeps in sight; older ones are collapsed (UI-641). */
export const FINISHED_SHOWN = 5;

/** A row's key in the window's memory of a thread's background work: `job:bg_1`, `subagent:Counter`. */
export function rowKey(row: BackgroundRow): string {
  return `${row.kind}:${row.id}`;
}

/** When this window first saw each row finished (row key → time): neither omp's job frames nor subagents carry it. */
export type FinishTimes = ReadonlyMap<string, number>;

/** `times` with the rows of `rows` that finished by `now`; unchanged when none is new. */
export function observeFinished(times: FinishTimes, rows: readonly BackgroundRow[], now: number): FinishTimes {
  let next: Map<string, number> | undefined;
  for (const row of rows) {
    const key = rowKey(row);
    if (row.state !== "running" && !times.has(key)) {
      next ??= new Map(times);
      next.set(key, now);
    }
  }
  return next ?? times;
}

/** What the window remembers of a thread's background work (UI-642, UI-643): the finished rows the user cleared and the
 * failed ones they opened, by {@link rowKey}. */
export interface BackgroundMemory {
  readonly cleared: readonly string[];
  readonly opened: readonly string[];
}

export const NO_MEMORY: BackgroundMemory = { cleared: [], opened: [] };

/** `omfp:background`'s rows in its sections (UI-641..UI-643). */
export interface BackgroundSections {
  /** What runs, as {@link backgroundRows} orders it. */
  readonly running: readonly BackgroundRow[];
  /** Finished rows in sight, most recently finished first: the failed ones not opened yet, and the
   * {@link FINISHED_SHOWN} most recent others. */
  readonly recent: readonly BackgroundRow[];
  /** The other finished rows, most recently finished first: the collapsed `Finished (N)` group. */
  readonly older: readonly BackgroundRow[];
}

/**
 * Splits `rows` into {@link BackgroundSections}: cleared finished rows are left out, finished ones are ordered by when
 * the window saw them finish (`finished`, newest first; equal times keep the order of `rows`).
 */
export function backgroundSections(
  rows: readonly BackgroundRow[],
  finished: FinishTimes,
  memory: BackgroundMemory,
): BackgroundSections {
  const cleared = new Set(memory.cleared);
  const opened = new Set(memory.opened);
  const running = rows.filter((row) => row.state === "running");
  const done = rows
    .filter((row) => row.state !== "running" && !cleared.has(rowKey(row)))
    .map((row, order) => ({ row, order, at: finished.get(rowKey(row)) ?? 0 }))
    .sort((a, b) => b.at - a.at || a.order - b.order)
    .map(({ row }) => row);
  const pinned = (row: BackgroundRow) => row.state === "failed" && !opened.has(rowKey(row));
  const others = done.filter((row) => !pinned(row));
  const shown = new Set(others.slice(0, FINISHED_SHOWN));
  return {
    running,
    recent: done.filter((row) => pinned(row) || shown.has(row)),
    older: others.slice(FINISHED_SHOWN),
  };
}

/** `memory` with every finished row of `sections` cleared (UI-642); the opened ones it no longer needs are dropped. */
export function clearFinished(memory: BackgroundMemory, sections: BackgroundSections): BackgroundMemory {
  const keys = [...sections.recent, ...sections.older].map(rowKey);
  return {
    cleared: [...new Set([...memory.cleared, ...keys])],
    opened: memory.opened.filter((key) => !keys.includes(key)),
  };
}

/** `memory` with `row` opened: a failed row then leaves the pinned ones (UI-643). */
export function openRow(memory: BackgroundMemory, row: BackgroundRow): BackgroundMemory {
  const key = rowKey(row);
  return row.state !== "failed" || memory.opened.includes(key)
    ? memory
    : { ...memory, opened: [...memory.opened, key] };
}

/** The steer that asks the agent to stop a job: the RPC has no command for it, the agent kills it (`proc://`). */
export function stopJobMessage(jobId: string): string {
  return `Stop background job ${jobId}.`;
}
