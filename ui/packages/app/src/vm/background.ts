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

/** When this window first saw each job run and saw it end (job id → times): omp's job frames carry no times. */
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
        durationMs: seen === undefined ? undefined : (seen.end ?? now) - seen.start,
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
      durationMs: row.durationMs,
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

/** The steer that asks the agent to stop a job: the RPC has no command for it, the agent kills it (`proc://`). */
export function stopJobMessage(jobId: string): string {
  return `Stop background job ${jobId}.`;
}
