/**
 * Hooks of a thread's background work (UI-081, UI-271..UI-274, UI-641..UI-643): the subagent roster (the state's
 * subagent frames merged with `thread.subagents`), and the jobs and subagents `omfp:background` and the header's badge
 * show, with stop and cancel, in sections (running, the recent finished, the older ones collapsed) with Clear finished.
 */

import type { Subagent, ToolRun } from "@omfp/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  type BackgroundMemory,
  type BackgroundRow,
  type BackgroundSections,
  backgroundRows,
  backgroundSections,
  clearFinished,
  type FinishTimes,
  type JobTimes,
  NO_MEMORY,
  observeFinished,
  observeJobs,
  openRow,
  runningInBackground,
  stopJobMessage,
} from "../vm/background.ts";
import { type SubagentRow, subagentRows } from "../vm/subagents.ts";
import { useReportError } from "./actions.ts";
import { useCall, useThread, useWindowState } from "./hooks.ts";
import { isObject } from "./json.ts";
import { windowKey } from "./window-store.ts";

const NO_SUBAGENTS: Readonly<Record<string, Subagent>> = {};
const NO_TOOLS: readonly ToolRun[] = [];

/**
 * The subagents of thread `threadId` (UI-081): every one its state knows, plus running ones only `thread.subagents`
 * reports, read again when a subagent starts or ends and on `refresh`. Only a running omp has a roster.
 */
export function useSubagentRows(threadId: string): { rows: SubagentRow[]; refresh: () => void } {
  const state = useThread(threadId)?.state;
  const call = useCall();
  const reportError = useReportError();
  const subagents = state?.subagents ?? NO_SUBAGENTS;
  const running = state?.process.state === "running";
  const [listed, setListed] = useState<unknown>(undefined);
  const [requested, setRequested] = useState(0);
  const known = Object.keys(subagents).join("\n");
  // biome-ignore lint/correctness/useExhaustiveDependencies: `known` and `requested` trigger a new read of the roster.
  useEffect(() => {
    if (!running) {
      return;
    }
    let current = true;
    call("thread.subagents", { threadId }).then((result) => {
      if (current) {
        setListed(result);
      }
    }, reportError);
    return () => {
      current = false;
    };
  }, [call, reportError, threadId, running, known, requested]);
  const rows = useMemo(() => subagentRows(subagents, listed), [subagents, listed]);
  const refresh = useCallback(() => setRequested((count) => count + 1), []);
  return { rows, refresh };
}

/** How many background jobs and subagents of thread `threadId` run (UI-274). */
export function useRunningInBackground(threadId: string): number {
  const tools = useThread(threadId)?.state.tools ?? NO_TOOLS;
  const { rows } = useSubagentRows(threadId);
  return runningInBackground(tools, rows);
}

/** How often running rows' durations tick. */
const TICK_MS = 1000;

/** How many threads' {@link BackgroundMemory} the window keeps; the least recently changed goes first. */
export const BACKGROUND_MEMORY_THREADS = 50;

function isKeys(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((key) => typeof key === "string");
}

/** The window's {@link BackgroundMemory} by thread id, kept across reloads (UI-642). */
export const BACKGROUND_MEMORY = windowKey<Readonly<Record<string, BackgroundMemory>>>(
  "backgroundMemory",
  {},
  (stored) => {
    if (!isObject(stored)) {
      return undefined;
    }
    const memory: Record<string, BackgroundMemory> = {};
    for (const [threadId, value] of Object.entries(stored)) {
      if (isObject(value) && isKeys(value.cleared) && isKeys(value.opened)) {
        memory[threadId] = { cleared: value.cleared, opened: value.opened };
      }
    }
    return memory;
  },
);

/** What `omfp:background` shows and does for thread `threadId`. */
export interface Background {
  readonly rows: readonly BackgroundRow[];
  /** The rows in their sections, without the cleared ones (UI-641..UI-643). */
  readonly sections: BackgroundSections;
  /** Jobs and subagents still running (UI-274). */
  readonly running: number;
  /** Asks the agent to stop job `jobId` with a steer (UI-272): the RPC has no kill command. */
  readonly stop: (jobId: string) => void;
  /** Cancels subagent `subagentId` (`thread.subagent_cancel`). */
  readonly cancel: (subagentId: string) => void;
  /** Hides every finished row in sight or collapsed from this window (UI-642). */
  readonly clearFinished: () => void;
  /** The user opened `row`: a failed one is no longer kept in sight for that (UI-643). */
  readonly opened: (row: BackgroundRow) => void;
}

/**
 * The background jobs and subagents of thread `threadId` (UI-271): job durations are measured from when this window
 * first saw the job (omp's frames carry no times), subagents' from their progress; running rows tick every second.
 */
export function useBackground(threadId: string): Background {
  const state = useThread(threadId)?.state;
  const tools = state?.tools ?? NO_TOOLS;
  // A steer while the agent works; while it is idle the message starts a turn, like Enter.
  const busy = state?.settled === false;
  const { rows: subagents } = useSubagentRows(threadId);
  const call = useCall();
  const reportError = useReportError();
  const times = useRef<JobTimes>(new Map());
  const finished = useRef<FinishTimes>(new Map());
  const [now, setNow] = useState(() => Date.now());
  const [stopping, setStopping] = useState<ReadonlySet<string>>(new Set());
  const [memories, setMemories] = useWindowState(BACKGROUND_MEMORY);
  times.current = observeJobs(times.current, tools, Date.now());
  const rows = backgroundRows(tools, subagents, stopping, times.current, now);
  finished.current = observeFinished(finished.current, rows, Date.now());
  const memory = memories[threadId] ?? NO_MEMORY;
  const sections = backgroundSections(rows, finished.current, memory);
  const running = rows.filter((row) => row.state === "running").length;
  useEffect(() => {
    if (running === 0) {
      return undefined;
    }
    const id = setInterval(() => setNow(Date.now()), TICK_MS);
    return () => clearInterval(id);
  }, [running]);
  const stop = useCallback(
    (jobId: string) => {
      setStopping((previous) => new Set([...previous, jobId]));
      call("thread.prompt", { threadId, text: stopJobMessage(jobId), ...(busy ? { behavior: "steer" } : {}) }).catch(
        (error: unknown) => {
          setStopping((previous) => new Set([...previous].filter((id) => id !== jobId)));
          reportError(error);
        },
      );
    },
    [busy, call, reportError, threadId],
  );
  const cancel = useCallback(
    (subagentId: string) => {
      call("thread.subagent_cancel", { threadId, subagentId }).catch(reportError);
    },
    [call, reportError, threadId],
  );
  const remember = (change: (memory: BackgroundMemory) => BackgroundMemory) =>
    setMemories((all) => {
      const before = all[threadId] ?? NO_MEMORY;
      const after = change(before);
      if (after === before) {
        return all;
      }
      // The thread moves to the end: the least recently changed ones go over the bound.
      const { [threadId]: _, ...others } = all;
      const kept = Object.entries(others).slice(-(BACKGROUND_MEMORY_THREADS - 1));
      return { ...Object.fromEntries(kept), [threadId]: after };
    });
  return {
    rows,
    sections,
    running,
    stop,
    cancel,
    clearFinished: () => remember((memory) => clearFinished(memory, sections)),
    opened: (row) => remember((memory) => openRow(memory, row)),
  };
}
