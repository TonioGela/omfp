/** Helpers shared by the reducer tests (port of `crates/omfp-core/tests/common/mod.rs`). */

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import {
  type AgentMessage,
  type AssistantMessage,
  type JsonValue,
  OMP_VERSION,
  replay,
  type ThreadEvent,
  type ThreadState,
} from "../src/index.ts";

const REPO = join(import.meta.dir, "../../../..");

/** Directory of the recorded thread fixtures of the pinned omp version. */
export const FIXTURE_DIR = join(REPO, "fixtures/threads", OMP_VERSION);

/** Directory of the golden states rendered by `crates/omfp-core/tests/golden_states.rs`. */
export const STATE_DIR = join(REPO, "fixtures/states", OMP_VERSION);

/** Names (file stems) of the `*.jsonl` files in `dir`, sorted. */
export function jsonlNames(dir: string): string[] {
  return readdirSync(dir)
    .filter((file) => file.endsWith(".jsonl"))
    .map((file) => file.slice(0, -".jsonl".length))
    .sort();
}

/** Non-blank lines of `dir/<name>.jsonl`. */
export function jsonlLines(dir: string, name: string): string[] {
  return readFileSync(join(dir, `${name}.jsonl`), "utf8")
    .split("\n")
    .filter((line) => line.trim() !== "");
}

/**
 * A `ThreadEvent` from JSON, as the daemon sends it. Not checked here: `reduce` validates its input at runtime, like
 * the Rust test helper that deserializes it.
 */
export function event(value: unknown): ThreadEvent {
  return value as ThreadEvent;
}

/** The recorded reducer inputs of fixture `name`, as JSON. */
export function fixtureJson(name: string): JsonValue[] {
  return jsonlLines(FIXTURE_DIR, name).map((line): JsonValue => JSON.parse(line));
}

/** The recorded reducer inputs of fixture `name`. */
export function fixture(name: string): ThreadEvent[] {
  return fixtureJson(name).map(event);
}

/** A `ThreadEvent` of kind `omp` wrapping the omp frame `frame`. */
export function omp(frame: JsonValue): ThreadEvent {
  return { kind: "omp", frame };
}

function field(value: unknown, key: string): unknown {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)[key]
    : undefined;
}

/** Index of the `nth` (0-based) event whose `kind`, frame `type` or assistant event `type` equals `type`. */
export function indexOf(events: readonly ThreadEvent[], type: string, nth: number): number {
  let seen = 0;
  for (const [index, item] of events.entries()) {
    const frame = field(item, "frame");
    const types = [field(item, "kind"), field(frame, "type"), field(field(frame, "assistantMessageEvent"), "type")];
    if (types.includes(type)) {
      if (seen === nth) {
        return index;
      }
      seen += 1;
    }
  }
  throw new Error(`no event #${nth} of type ${type}`);
}

/** State after applying `events[..=index]`. */
export function stateThrough(events: readonly ThreadEvent[], index: number): ThreadState {
  return replay(events.slice(0, index + 1));
}

/** Concatenated text blocks of an assistant message. */
export function assistantText(message: AssistantMessage): string {
  return (message.content ?? []).map((block) => (block.type === "text" ? (block.text ?? "") : "")).join("");
}

/** Concatenated text of any user or assistant message. */
export function messageText(message: AgentMessage): string {
  switch (message.role) {
    case "assistant":
      return assistantText(message);
    case "user":
      if (typeof message.content === "string") {
        return message.content;
      }
      return (message.content ?? []).map((block) => (block.type === "text" ? (block.text ?? "") : "")).join("");
    default:
      return "";
  }
}

/** Text of the streaming message, or `undefined` when nothing streams. */
export function streamingText(state: ThreadState): string | undefined {
  return state.streaming === null ? undefined : assistantText(state.streaming.message);
}

/** `value` with every object's keys sorted recursively. */
function sorted(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(sorted);
  }
  if (typeof value === "object" && value !== null) {
    const entries = Object.entries(value).sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0));
    return Object.fromEntries(entries.map(([key, item]) => [key, sorted(item)]));
  }
  return value;
}

/** Canonical JSON text of `value`: keys sorted, compact; `1.0` and `1` are the same number. */
export function canonical(value: unknown): string {
  return JSON.stringify(sorted(value));
}

/** Freezes `value` and everything reachable from it, so any later mutation throws. */
export function deepFreeze<T>(value: T): T {
  if (typeof value === "object" && value !== null && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const item of Object.values(value)) {
      deepFreeze(item);
    }
  }
  return value;
}
