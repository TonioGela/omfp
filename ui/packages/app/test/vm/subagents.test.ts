/** L0: the subagent roster and transcripts (UI-081), from the `task` fixture recorded from the pinned omp. */

import { describe, expect, test } from "bun:test";
import { type AgentMessage, replay } from "@omfp/core";
import { appendTranscriptPage, EMPTY_TRANSCRIPT, subagentRows, transcriptLines } from "../../src/vm/subagents.ts";
import { fixtureEvents, isFrame } from "../widgets/chat/fixtures.ts";

const events = fixtureEvents("task_tool_call");

describe("subagentRows", () => {
  // spec: UI-081, PRT-055
  test("from the thread state: running while omp reports progress, then its lifecycle's end", () => {
    const firstProgress = events.findIndex(isFrame("subagent_progress"));
    const running = subagentRows(replay(events.slice(0, firstProgress + 1)).subagents, undefined);
    expect(running.map((row) => [row.id, row.agent, row.status, row.live])).toEqual([
      ["Counter", "task", "running", true],
    ]);
    expect(running[0]?.task).toBe("Count the functions in src/lib.rs");

    const [done] = subagentRows(replay(events).subagents, { subagents: [] });
    expect(done).toMatchObject({ id: "Counter", status: "completed", live: false, activity: "Reading the library" });
    expect(done?.meta).toBe("1 tool · fake/scripted");
  });

  // spec: UI-081
  test("running subagents only `thread.subagents` knows join the roster, by spawn index", () => {
    const known = replay(events).subagents;
    const listed = {
      subagents: [
        { id: "Later", index: 3, agent: "scout", status: "running", assignment: "Look around", lastUpdate: 0 },
        { id: "Early", index: 0, agent: "task", status: "running", task: "First", progress: { toolCount: 2 } },
        { index: 9 },
      ],
    };
    const rows = subagentRows(known, listed);
    expect(rows.map((row) => [row.id, row.status, row.task])).toEqual([
      ["Counter", "completed", "Count the functions in src/lib.rs"],
      ["Early", "running", "First"],
      ["Later", "running", "Look around"],
    ]);
    expect(rows[1]?.meta).toBe("2 tools");
  });
});

const user = { role: "user", content: "Count them", timestamp: 0 } as AgentMessage;
const reply = {
  role: "assistant",
  content: [
    { type: "text", text: "Reading first." },
    { type: "toolCall", id: "c1", name: "read", arguments: {}, intent: "Reading the library" },
  ],
} as AgentMessage;

describe("subagent transcripts", () => {
  // spec: UI-081
  test("pages append from `fromByte`; a reset replaces; an empty page changes nothing", () => {
    const first = appendTranscriptPage(EMPTY_TRANSCRIPT, { messages: [user], nextByte: 120, reset: false });
    const second = appendTranscriptPage(first, { messages: [reply], nextByte: 300, reset: false });
    expect(second).toEqual({ messages: [user, reply], nextByte: 300 });
    expect(appendTranscriptPage(second, { messages: [], nextByte: 300, reset: false })).toBe(second);
    expect(appendTranscriptPage(second, { messages: [reply], nextByte: 90, reset: true })).toEqual({
      messages: [reply],
      nextByte: 90,
    });
    expect(appendTranscriptPage(second, { nope: true })).toBe(second);
  });

  // spec: UI-081
  test("lines: the task, the subagent's text and one line per tool call", () => {
    expect(transcriptLines([user, reply, { role: "toolResult", toolName: "read" } as AgentMessage])).toEqual([
      { role: "user", text: "Count them" },
      { role: "assistant", text: "Reading first." },
      { role: "tool", text: "→ read: Reading the library" },
    ]);
  });
});
