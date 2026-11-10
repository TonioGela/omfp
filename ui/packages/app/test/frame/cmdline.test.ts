/** L1: the `:` line's parsing and completion (UI-095) and what deserves a notification (UI-096). */

import { describe, expect, test } from "bun:test";
import { completions, parseCommandLine } from "../../src/frame/cmdline.ts";
import { notificationsFor } from "../../src/frame/notifications.ts";
import type { Command } from "../../src/sdk/index.ts";
import { thread } from "../support/fixtures.ts";

const noop = () => {};
const commands: Command[] = [
  { id: "thread.archive", title: "Archive", aliases: ["archive"], run: noop },
  { id: "thread.unarchive", title: "Unarchive", aliases: ["unarchive"], enabled: () => false, run: noop },
  { id: "theme.set", title: "Theme", aliases: ["theme"], args: () => ["system", "light", "dark"], run: noop },
  { id: "nav.next", title: "Next message", scope: "transcript", run: noop },
  { id: "nav.next", title: "Next thread", run: noop },
];

describe("command line", () => {
  // spec: UI-095
  test("a line is a command name and its arguments", () => {
    expect(parseCommandLine("  theme   dark ")).toEqual({ name: "theme", args: ["dark"] });
    expect(parseCommandLine("rename a new title")).toEqual({ name: "rename", args: ["a", "new", "title"] });
    expect(parseCommandLine("   ")).toBeUndefined();
  });

  // spec: UI-095
  test("completion lists enabled commands by alias or id, once each, then their arguments", () => {
    expect(completions("", commands).map((option) => option.value)).toEqual(["archive", "theme", "nav.next"]);
    expect(completions("nav", commands).map((option) => option.title)).toEqual(["Next thread"]);
    expect(completions("thread.a", commands).map((option) => option.value)).toEqual(["thread.archive"]);
    expect(completions("un", commands)).toEqual([]);
    expect(completions("theme ", commands).map((option) => option.value)).toEqual([
      "theme system",
      "theme light",
      "theme dark",
    ]);
    expect(completions("theme l", commands).map((option) => option.value)).toEqual(["theme light"]);
    expect(completions("nope x", commands)).toEqual([]);
  });
});

describe("notifications", () => {
  // spec: UI-096
  test("a thread starting to need the user, failing or finishing a turn is notified; new threads are not", () => {
    const before = new Map([
      ["t1", thread("t1", "ask", "w")],
      ["t2", thread("t2", "", "w", { settled: false })],
      ["t3", thread("t3", "fail", "w")],
      ["t4", thread("t4", "old", "w", { settled: false })],
    ]);
    const after = new Map([
      ["t1", thread("t1", "ask", "w", { needsAttention: true })],
      ["t2", thread("t2", "", "w")],
      ["t3", thread("t3", "fail", "w", { process: { state: "failed", message: "x" } })],
      ["t4", thread("t4", "old", "w", { archivedAt: 1 })],
      ["t5", thread("t5", "new", "w", { needsAttention: true })],
    ]);
    expect(notificationsFor(before, after).map((item) => [item.threadId, item.kind, item.title, item.body])).toEqual([
      ["t1", "attention", "ask", "Needs your input"],
      ["t2", "finished", "New thread", "Finished its turn"],
      ["t3", "failed", "fail", "Failed"],
    ]);
    expect(notificationsFor(after, after)).toEqual([]);
  });
});
