/**
 * Widget data sources (docs/specs/08-agent-control.md): state the daemon keeps per widget instance ({@link useStored},
 * AGT-018), daemon jobs ({@link useJob}, AGT-019) and the agent's tool runs ({@link useToolRun}, AGT-020).
 */

import type { ToolRun, ToolStatus } from "@omfp/core";
import { type JobState, jobTopic } from "@omfp/protocol";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { strings } from "../strings/index.ts";
import { resultText } from "../vm/tool.ts";
import { reportError } from "./actions.ts";
import { useSnapshot, useThread, useTopic, useViews, useWindowState } from "./hooks.ts";
import { useHost } from "./host.tsx";
import { SELECTED_THREAD } from "./keys.ts";
import { useWidget } from "./widget.ts";

/** A new value, or an updater of the current one; `null` deletes the key. */
export type StoredUpdate<T> = T | null | ((current: T) => T | null);

/**
 * `[value, set, loaded]`: the JSON value this widget instance stored under `key` in its workspace (DMN-086), else
 * `initial`; the same in every window and pushed live. `set` stores a value or the result of an updater
 * (`widgets.state_set`), `null` deletes the key; failures go to the frame's error list. `loaded` is `false` until the
 * workspace's views state arrived, while `value` is only `initial`. The stored JSON is not checked against `T`.
 */
export function useStored<T>(key: string, initial: T): [T, (update: StoredUpdate<T>) => void, boolean] {
  const host = useHost();
  const { instance, workspaceId } = useWidget();
  const views = useViews(workspaceId);
  const keys = views?.stored[instance];
  // JSON from the daemon, trusted to be what this widget stored.
  const value = keys !== undefined && Object.hasOwn(keys, key) ? (keys[key] as T) : initial;
  // The value updaters start from: the pushed value, or the one set last while its push is on the way.
  const latest = useRef(value);
  useEffect(() => {
    latest.current = value;
  }, [value]);
  const set = useCallback(
    (update: StoredUpdate<T>) => {
      if (workspaceId === undefined) {
        reportError(host, strings.widgets.noWorkspace);
        return;
      }
      const next = typeof update === "function" ? (update as (current: T) => T | null)(latest.current) : update;
      latest.current = next ?? initial;
      host.client
        .call("widgets.state_set", { workspaceId, instance, key, value: next })
        .catch((error: unknown) => reportError(host, error));
    },
    [host, workspaceId, instance, key, initial],
  );
  return [value, set, views !== undefined];
}

/** A daemon job of this widget instance (AGT-019). */
export interface Job {
  /** Output lines (stdout and stderr interleaved as they came), oldest first; empty until the job is known. */
  readonly lines: readonly string[];
  readonly running: boolean;
  /** The exit code once the job exited normally, else `null`. */
  readonly exitCode: number | null;
  /** The signal that ended the job, else `null`. */
  readonly signal: number | null;
  /** The job as the daemon reports it (`job/<id>`), `undefined` until known. */
  readonly state: JobState | undefined;
  /** Kills the job when it runs, then starts a new one. */
  restart(): void;
  /** Stops the running job (SIGTERM, then SIGKILL after 2 s, DMN-081). */
  kill(): void;
}

const NO_LINES: readonly string[] = [];

/**
 * Runs `command` in the workspace (`jobs.start`, DMN-080) once per widget instance: the job's id is stored with
 * {@link useStored} under `job:<command>`; while nothing is stored or the daemon no longer knows the stored job, a job
 * is started; otherwise the stored job is followed (`job/<id>`), running or finished, so reloads, other windows and
 * moving the widget never start a second one.
 */
export function useJob(command: string): Job {
  const host = useHost();
  const { workspaceId } = useWidget();
  const [jobId, setJobId, loaded] = useStored<string | null>(`job:${command}`, null);
  const topic = useTopic(jobId === null ? undefined : jobTopic(jobId));
  const state = useSnapshot((snapshot) => (jobId === null ? undefined : snapshot.jobs.get(jobId)));
  // The stored id (or `null`) a start was made for: no second start until the stored id changes.
  const startedFor = useRef<string | null | undefined>(undefined);

  const start = useCallback(
    (replacing: string | null) => {
      if (workspaceId === undefined) {
        return;
      }
      startedFor.current = replacing;
      host.client
        .call("jobs.start", { workspaceId, command })
        .then((job) => setJobId(job.id))
        .catch((error: unknown) => reportError(host, error));
    },
    [host, workspaceId, command, setJobId],
  );

  const gone = topic?.sync.kind === "failed" && topic.sync.error.code === "not_found";
  useEffect(() => {
    if (loaded && (jobId === null || gone) && startedFor.current !== jobId) {
      start(jobId);
    }
  }, [loaded, jobId, gone, start]);

  const running = state?.job.status.state === "running";
  const restart = useCallback(() => {
    if (jobId !== null && running) {
      host.client
        .call("jobs.kill", { jobId })
        .then(() => start(jobId))
        .catch((error: unknown) => reportError(host, error));
    } else {
      start(jobId);
    }
  }, [host, jobId, running, start]);
  const kill = useCallback(() => {
    if (jobId !== null) {
      host.client.call("jobs.kill", { jobId }).catch((error: unknown) => reportError(host, error));
    }
  }, [host, jobId]);

  const jobLines = state?.lines;
  const lines = useMemo(() => jobLines?.map((line) => line.text) ?? NO_LINES, [jobLines]);
  const status = state?.job.status;
  return {
    lines,
    running,
    exitCode: status?.state === "exited" ? (status.code ?? null) : null,
    signal: status?.state === "exited" ? (status.signal ?? null) : null,
    state,
    restart,
    kill,
  };
}

/** Which tool run {@link useToolRun} follows. */
export type ToolRunTarget =
  | string
  | {
      /** Matches the run's command (`arguments.command` when it is a string, else its JSON arguments); a string
       * matches when the command contains it. */
      readonly match: RegExp | string;
      /** Default: the thread this window shows. */
      readonly threadId?: string;
    };

/** One tool run of a thread (AGT-020). */
export interface ToolRunInfo {
  readonly toolCallId: string;
  readonly toolName: string;
  readonly status: ToolStatus;
  /** The latest partial output while it runs (omp streams the bash output's latest tail), the result's text once it
   * ended. */
  readonly output: string;
  /** {@link ToolRunInfo.output} split into lines (a final line break adds no empty line). */
  readonly lines: readonly string[];
  readonly isError: boolean;
}

function commandOf(run: ToolRun): string {
  const { args } = run;
  if (typeof args === "object" && args !== null && !Array.isArray(args) && typeof args.command === "string") {
    return args.command;
  }
  return JSON.stringify(args);
}

function matches(run: ToolRun, match: RegExp | string): boolean {
  const command = commandOf(run);
  if (typeof match === "string") {
    return command.includes(match);
  }
  match.lastIndex = 0;
  return match.test(command);
}

/** The tool run `toolCallId`, or the latest whose command matches, of a thread (the selected one by default),
 * following `thread/<id>`; `undefined` while there is none. */
export function useToolRun(target: ToolRunTarget): ToolRunInfo | undefined {
  const [selected] = useWindowState(SELECTED_THREAD);
  const threadId = typeof target === "string" ? selected : (target.threadId ?? selected);
  const tools = useThread(threadId)?.state.tools;
  // Runs are immutable reducer objects: the result below changes only when the run does.
  const run =
    typeof target === "string"
      ? tools?.find((candidate) => candidate.tool_call_id === target)
      : tools?.findLast((candidate) => matches(candidate, target.match));
  return useMemo(() => {
    if (run === undefined) {
      return undefined;
    }
    const ended = run.status === "succeeded" || run.status === "failed";
    const value = ended ? run.result : run.partial_result;
    const output = value === null ? "" : resultText(value);
    return {
      toolCallId: run.tool_call_id,
      toolName: run.tool_name,
      status: run.status,
      output,
      // A final line break ends the last line, it does not start an empty one.
      lines: output === "" ? [] : output.replace(/\n$/, "").split("\n"),
      isError: run.status === "failed",
    };
  }, [run]);
}
