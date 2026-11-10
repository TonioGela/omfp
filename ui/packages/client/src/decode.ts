/**
 * Validation of what the daemon sends: server frames, the `system.hello` and `subscribe` results, and topic snapshots.
 * A value that passes has the shape of its TypeScript type down to the fields the client and the reducer branch on;
 * nested omp wire objects are left to the reducer, which accepts any JSON.
 */

import type { ThreadEvent, ThreadState } from "@omfp/core";
import {
  type ErrorBody,
  type ErrorCode,
  type EventFrame,
  type HelloResult,
  type IndexEvent,
  type IndexSnapshot,
  type ParsedTopic,
  parseTopic,
  type RepoStatus,
  type ServerFrame,
  type SubscribeResult,
  type TopicEvent,
  type WorkspaceEvent,
} from "@omfp/protocol";
import { isObject } from "./json.ts";

/** A decoded value, or why it does not decode. */
export type Decoded<T> = { ok: true; value: T } | { ok: false; error: string };

function invalid<T>(error: string): Decoded<T> {
  return { ok: false, error };
}

const ERROR_CODES = {
  unauthorized: true,
  bad_request: true,
  not_found: true,
  conflict: true,
  already_resolved: true,
  limit_reached: true,
  version_mismatch: true,
  thread_failed: true,
  omp_error: true,
  git_error: true,
  internal: true,
} satisfies Record<ErrorCode, true>;

function isErrorCode(value: unknown): value is ErrorCode {
  return typeof value === "string" && Object.hasOwn(ERROR_CODES, value);
}

function isSeq(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isOptionalString(value: unknown): boolean {
  return value === undefined || value === null || typeof value === "string";
}

function isNullableObject(value: unknown): boolean {
  return value === null || isObject(value);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function hasId(value: unknown): boolean {
  return isObject(value) && typeof value.id === "string";
}

function decodeErrorBody(value: unknown): Decoded<ErrorBody> {
  if (!isObject(value) || !isErrorCode(value.code) || typeof value.message !== "string") {
    return invalid("`error` needs a known `code` and a string `message`");
  }
  if (!isOptionalString(value.ompCode)) {
    return invalid("`error.ompCode` must be a string");
  }
  const body: ErrorBody = { code: value.code, message: value.message };
  if (typeof value.ompCode === "string") {
    body.ompCode = value.ompCode;
  }
  if (value.details !== undefined) {
    body.details = value.details;
  }
  return { ok: true, value: body };
}

const INDEX_EVENTS: Record<IndexEvent["kind"], (event: Record<string, unknown>) => boolean> = {
  repository_upserted: (event) => hasId(event.repository),
  repository_removed: (event) => typeof event.repositoryId === "string",
  workspace_upserted: (event) => hasId(event.workspace),
  workspace_removed: (event) => typeof event.workspaceId === "string",
  thread_upserted: (event) => hasId(event.thread),
  thread_removed: (event) => typeof event.threadId === "string",
};

const THREAD_EVENTS: Record<ThreadEvent["kind"], (event: Record<string, unknown>) => boolean> = {
  omp: (event) => isObject(event.frame),
  history: (event) => isObject(event.session) && Array.isArray(event.messages),
  process: (event) => isObject(event.state) && typeof event.state.state === "string",
  interaction_resolved: (event) =>
    typeof event.id === "string" && isObject(event.resolution) && typeof event.resolution.by === "string",
  unknown: (event) => "raw" in event,
};

function isRepoStatusList(value: unknown): value is RepoStatus[] {
  return (
    Array.isArray(value) &&
    value.every((repo) => isObject(repo) && typeof repo.name === "string" && isObject(repo.status))
  );
}

const WORKSPACE_EVENTS: Record<WorkspaceEvent["kind"], (event: Record<string, unknown>) => boolean> = {
  status: (event) => isRepoStatusList(event.repos),
};

function checkKind(table: Record<string, (event: Record<string, unknown>) => boolean>, event: unknown): boolean {
  if (!isObject(event) || typeof event.kind !== "string" || !Object.hasOwn(table, event.kind)) {
    return false;
  }
  return table[event.kind]?.(event) === true;
}

const EVENT_TABLES: Record<
  ParsedTopic["kind"],
  Record<string, (event: Record<string, unknown>) => boolean> | undefined
> = {
  index: INDEX_EVENTS,
  thread: THREAD_EVENTS,
  workspace: WORKSPACE_EVENTS,
  terminal: undefined,
};

/** Decodes the `event` of a frame on `topic`: its kind must belong to the topic's kind. */
function decodeTopicEvent(topic: ParsedTopic, event: unknown): Decoded<TopicEvent> {
  const table = EVENT_TABLES[topic.kind];
  if (table === undefined) {
    return invalid("terminal output travels in binary frames, not in event frames");
  }
  // Checked against the topic's event table.
  return checkKind(table, event) ? { ok: true, value: event as TopicEvent } : invalid(`not a ${topic.kind} event`);
}

/** Decodes a JSON-parsed daemon frame (PRT-010, PRT-022, PRT-023). */
export function decodeServerFrame(value: unknown): Decoded<ServerFrame> {
  if (!isObject(value)) {
    return invalid("a frame must be a JSON object");
  }
  switch (value.type) {
    case "response": {
      const { id, ok } = value;
      if (typeof id !== "string" || typeof ok !== "boolean") {
        return invalid("a response needs a string `id` and a boolean `ok`");
      }
      if (ok) {
        if (value.error !== undefined) {
          return invalid("a successful response has no `error`");
        }
        return { ok: true, value: { type: "response", id, ok, result: value.result ?? null } };
      }
      if (value.error === undefined || value.result !== undefined) {
        return invalid("a failed response needs an `error` and no `result`");
      }
      const error = decodeErrorBody(value.error);
      return error.ok ? { ok: true, value: { type: "response", id, ok, error: error.value } } : error;
    }
    case "event": {
      const { topic, epoch, seq } = value;
      const parsed = typeof topic === "string" ? parseTopic(topic) : undefined;
      if (typeof topic !== "string" || parsed === undefined || typeof epoch !== "string" || !isSeq(seq)) {
        return invalid("an event needs a valid `topic`, a string `epoch` and a non-negative integer `seq`");
      }
      const event = decodeTopicEvent(parsed, value.event);
      if (!event.ok) {
        return invalid(`invalid event for topic ${topic}: ${event.error}`);
      }
      const frame: EventFrame = { type: "event", topic, epoch, seq, event: event.value };
      return { ok: true, value: frame };
    }
    case "resync_required": {
      const { topic } = value;
      if (typeof topic !== "string" || parseTopic(topic) === undefined) {
        return invalid("`resync_required` needs a valid `topic`");
      }
      return { ok: true, value: { type: "resync_required", topic } };
    }
    default:
      return invalid("unknown frame `type`");
  }
}

/** Decodes the result of `system.hello` (PRT-005). */
export function decodeHelloResult(value: unknown): Decoded<HelloResult> {
  if (
    !isObject(value) ||
    typeof value.protocol !== "number" ||
    typeof value.daemon !== "string" ||
    typeof value.omp !== "string" ||
    !isStringArray(value.warnings)
  ) {
    return invalid("expected `protocol`, `daemon`, `omp` and `warnings`");
  }
  return {
    ok: true,
    value: { protocol: value.protocol, daemon: value.daemon, omp: value.omp, warnings: value.warnings },
  };
}

/** Decodes the result of `subscribe` (PRT-020). */
export function decodeSubscribeResult(value: unknown): Decoded<SubscribeResult> {
  if (isObject(value) && typeof value.epoch === "string") {
    const { epoch } = value;
    if (value.mode === "replay" && isSeq(value.fromSeq)) {
      return { ok: true, value: { mode: "replay", epoch, fromSeq: value.fromSeq } };
    }
    if (value.mode === "snapshot" && isSeq(value.seq) && "state" in value) {
      return { ok: true, value: { mode: "snapshot", epoch, seq: value.seq, state: value.state } };
    }
  }
  return invalid("expected a `replay` ({epoch, fromSeq}) or a `snapshot` ({epoch, seq, state})");
}

/** Decodes the state of an `index` snapshot. */
export function decodeIndexSnapshot(value: unknown): Decoded<IndexSnapshot> {
  if (
    isObject(value) &&
    [value.repositories, value.workspaces, value.threads].every((list) => Array.isArray(list) && list.every(hasId))
  ) {
    // Every list holds objects with a string `id`, the key the model files them under.
    return { ok: true, value: value as IndexSnapshot };
  }
  return invalid("expected `repositories`, `workspaces` and `threads`, each a list of objects with an `id`");
}

const THREAD_STATE_FIELDS: Record<keyof ThreadState, (field: unknown) => boolean> = {
  process: (field) => isObject(field) && typeof field.state === "string",
  session: isNullableObject,
  settled: (field) => typeof field === "boolean",
  transcript: Array.isArray,
  streaming: isNullableObject,
  tools: Array.isArray,
  queue: isObject,
  interactions: Array.isArray,
  presentation: isObject,
  commands: Array.isArray,
  prompts: isObject,
  subagents: isObject,
  notices: Array.isArray,
};

/** Decodes the state of a `thread/<id>` snapshot: the daemon's `ThreadState` (PRT-021). */
export function decodeThreadState(value: unknown): Decoded<ThreadState> {
  const state = value;
  if (!isObject(state)) {
    return invalid("a thread state must be an object");
  }
  for (const [field, check] of Object.entries(THREAD_STATE_FIELDS)) {
    if (!check(state[field])) {
      return invalid(`invalid or missing thread state field \`${field}\``);
    }
  }
  // Every field checked above; nested omp wire objects are the reducer's business.
  return { ok: true, value: value as ThreadState };
}

/** Decodes the state of a `workspace/<id>` snapshot: the git status of each repository. */
export function decodeRepoStatusList(value: unknown): Decoded<RepoStatus[]> {
  return isRepoStatusList(value)
    ? { ok: true, value }
    : invalid("expected a list of `{ name, status }` repository statuses");
}
