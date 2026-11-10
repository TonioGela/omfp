/** Recorded thread fixtures (TST-012) for the conversation view models and widgets. */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { OMP_VERSION, replay, type ThreadEvent, type ThreadState } from "@omfp/core";
import type { ThreadSummary } from "@omfp/protocol";
import { isObject } from "../../../src/vm/tool.ts";

export type FixtureName =
  | "text_reply"
  | "bash_tool_call"
  | "approval_granted"
  | "ask_answered"
  | "follow_up_then_abort"
  | "restart_with_history";

/** The `ThreadEvent`s of `fixtures/threads/<omp>/<name>.jsonl`. */
export function fixtureEvents(name: FixtureName): ThreadEvent[] {
  const path = join(import.meta.dir, "../../../../../../fixtures/threads", OMP_VERSION, `${name}.jsonl`);
  return readFileSync(path, "utf8")
    .split("\n")
    .filter((line) => line.trim() !== "")
    .map((line) => JSON.parse(line) as ThreadEvent);
}

/** The events before the first one `stop` accepts (all of them when none does). */
export function eventsUntil(name: FixtureName, stop: (event: ThreadEvent) => boolean): ThreadEvent[] {
  const events = fixtureEvents(name);
  const end = events.findIndex(stop);
  return end === -1 ? events : events.slice(0, end);
}

/** The state after the whole fixture. */
export function fixture(name: FixtureName): ThreadState {
  return replay(fixtureEvents(name));
}

/** The state after the events before the first one `stop` accepts. */
export function fixtureUntil(name: FixtureName, stop: (event: ThreadEvent) => boolean): ThreadState {
  return replay(eventsUntil(name, stop));
}

/** The state after the fixture's first `count` events. */
export function fixturePrefix(name: FixtureName, count: number): ThreadState {
  return replay(fixtureEvents(name).slice(0, count));
}

/** An omp frame of type `type` (and, for `message_update`, assistant event `delta`). */
export function isFrame(type: string, delta?: string): (event: ThreadEvent) => boolean {
  return (event) => {
    if (event.kind !== "omp") {
      return false;
    }
    const frame = event.frame;
    if (!isObject(frame) || frame.type !== type) {
      return false;
    }
    const update = frame.assistantMessageEvent;
    return delta === undefined || (isObject(update) && update.type === delta);
  };
}

/** A thread summary for the index. */
export function summary(id: string, settled = true): ThreadSummary {
  return {
    id,
    title: id,
    titleSource: "set",
    tags: [],
    workspaceId: "01W",
    process: { state: "running" },
    settled,
    needsAttention: false,
    provenance: { createdBy: { kind: "user" }, client: "test" },
    createdAt: 1,
    updatedAt: 1,
  };
}
