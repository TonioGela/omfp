/** Test helpers shared by the model and session tests: recorded fixtures and DTO builders. */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { OMP_VERSION, type ThreadEvent } from "@omfp/core";
import {
  type EventFrame,
  INDEX_TOPIC,
  type IndexEvent,
  type IndexSnapshot,
  type ThreadSummary,
  threadTopic,
  type Workspace,
} from "@omfp/protocol";

/** The `ThreadEvent`s of a recorded fixture (TST-012). */
export function fixture(name: string): ThreadEvent[] {
  const path = join(import.meta.dir, "../../../../fixtures/threads", OMP_VERSION, `${name}.jsonl`);
  return readFileSync(path, "utf8")
    .split("\n")
    .filter((line) => line.trim() !== "")
    .map((line) => JSON.parse(line) as ThreadEvent);
}

export const FIXTURES = [
  "text_reply",
  "bash_tool_call",
  "approval_granted",
  "ask_answered",
  "follow_up_then_abort",
  "restart_with_history",
] as const;

export function threadFrame(id: string, epoch: string, seq: number, event: ThreadEvent): EventFrame {
  return { type: "event", topic: threadTopic(id), epoch, seq, event };
}

export function indexFrame(epoch: string, seq: number, event: IndexEvent): EventFrame {
  return { type: "event", topic: INDEX_TOPIC, epoch, seq, event };
}

export function summary(id: string, title: string): ThreadSummary {
  return {
    id,
    title,
    titleSource: "set",
    tags: [],
    workspaceId: "01W",
    process: { state: "stopped" },
    settled: true,
    needsAttention: false,
    provenance: { createdBy: { kind: "user" }, client: "test" },
    createdAt: 1,
    updatedAt: 1,
  };
}

export function workspace(id: string): Workspace {
  return {
    id,
    name: id.toLowerCase(),
    path: `/ws/${id}`,
    owned: true,
    repos: [],
    threadIds: [],
    provenance: { createdBy: { kind: "user" } },
    createdAt: 1,
  };
}

/** An `index` snapshot without a user theme or settings. */
export function indexState(parts: Partial<IndexSnapshot> = {}): IndexSnapshot {
  return {
    workspaces: [],
    threads: [],
    theme: { css: null },
    settings: { ui: {}, keymap: [], error: null },
    ...parts,
  };
}
