/** L1: the thread list view-model (port of the Dioxus `vm/threads.rs` tests). */

import { describe, expect, test } from "bun:test";
import type { ThreadSummary, Workspace, WorkspaceRepo } from "@omfp/protocol";
import {
  displayTitle,
  type Filter,
  filterOptions,
  type Grouping,
  groupThreads,
  isUnread,
  listOrder,
  moveSelection,
  NO_FILTER,
  nextNeedingAttention,
  threadStatus,
  updateSeen,
  waitingCount,
  workspaceChips,
} from "../../src/vm/threads.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";

function index(): { workspaces: Workspace[]; threads: ThreadSummary[] } {
  const linked: WorkspaceRepo = {
    ...repo("w-checkout", "be", "main"),
    kind: "link",
    target: "/src/be",
    canonicalPath: "/src/be",
  };
  const workspaces = [
    { ...workspace("w-checkout", [repo("w-checkout", "fe", "feat/checkout"), linked]), name: "checkout" },
    { ...workspace("w-flaky", [repo("w-flaky", "be", "flaky")]), name: "flaky" },
    { ...workspace("w-scratch"), name: "scratch" },
  ];
  const threads = [
    thread("t-checkout", "checkout-v2", "w-checkout", { tags: ["release"] }),
    thread("t-flaky", "flaky-tests", "w-flaky", { tags: ["ci", "release"] }),
    thread("t-scratch", "scratch", "w-scratch"),
  ];
  return { workspaces, threads };
}

function groups(grouping: Grouping, filter: Filter = NO_FILTER): Array<[string, readonly string[]]> {
  const { workspaces, threads } = index();
  return groupThreads(threads, workspaces, grouping, filter).map((group) => [group.label, group.threadIds]);
}

describe("thread list", () => {
  // spec: UI-020, UI-011
  test("status puts attention before the process and the settled flag", () => {
    const summary = thread("t", "t", "w");
    expect(threadStatus(summary)).toBe("idle");
    expect(threadStatus({ ...summary, settled: false })).toBe("running");
    expect(threadStatus({ ...summary, settled: false, needsAttention: true })).toBe("awaiting");
    expect(threadStatus({ ...summary, process: { state: "failed", message: "boom" } })).toBe("failed");
    expect(threadStatus({ ...summary, process: { state: "stopped" } })).toBe("stopped");
    expect(threadStatus({ ...summary, process: { state: "terminal" } })).toBe("terminal");
  });

  // spec: UI-020
  test("a thread without a title yet shows the placeholder title", () => {
    expect(displayTitle(thread("t", "", "w"))).toBe("New thread");
    expect(displayTitle(thread("t", "checkout", "w"))).toBe("checkout");
  });

  // spec: UI-020, UI-011
  test("chips show the workspace then each repository with kind and branch", () => {
    const { workspaces, threads } = index();
    const chips = workspaceChips(threads[0]?.workspaceId ?? "", workspaces);
    expect(chips.map((chip) => [chip.label, chip.kind])).toEqual([
      ["checkout", "workspace"],
      ["fe:feat/checkout", "worktree"],
      ["be:main", "link"],
    ]);
    expect(workspaceChips("w-unknown", workspaces)).toEqual([]);
  });

  // spec: UI-022, UI-011
  test("grouping by workspace lists each thread under its workspace", () => {
    expect(groups("workspace")).toEqual([
      ["checkout", ["t-checkout"]],
      ["flaky", ["t-flaky"]],
      ["scratch", ["t-scratch"]],
    ]);
  });

  // spec: UI-022, UI-011
  test("grouping by repository lists a thread under each repository of its workspace", () => {
    expect(groups("repository")).toEqual([
      ["be", ["t-checkout", "t-flaky"]],
      ["fe", ["t-checkout"]],
      ["No repository", ["t-scratch"]],
    ]);
  });

  // spec: UI-022, UI-011
  test("grouping by tag and none", () => {
    expect(groups("tag")).toEqual([
      ["ci", ["t-flaky"]],
      ["release", ["t-checkout", "t-flaky"]],
      ["Untagged", ["t-scratch"]],
    ]);
    expect(groups("none")).toEqual([["Threads", ["t-checkout", "t-flaky", "t-scratch"]]]);
  });

  // spec: UI-022
  test("list order takes each thread once, at its first group", () => {
    const { workspaces, threads } = index();
    expect(listOrder(groupThreads(threads, workspaces, "repository", NO_FILTER))).toEqual([
      "t-checkout",
      "t-flaky",
      "t-scratch",
    ]);
  });

  // spec: UI-095
  test("j/k, gg/G and half pages move through the list order, clamped at both ends", () => {
    const order = ["a", "b", "c"];
    expect(moveSelection(order, undefined, 1)).toBe("a");
    expect(moveSelection(order, undefined, -1)).toBe("c");
    expect(moveSelection(order, "a", 1)).toBe("b");
    expect(moveSelection(order, "a", -1)).toBe("a");
    expect(moveSelection(order, "b", 10)).toBe("c");
    expect(moveSelection(order, "c", "top")).toBe("a");
    expect(moveSelection(order, "a", "bottom")).toBe("c");
    expect(moveSelection(order, "gone", 1)).toBe("a");
    expect(moveSelection([], "a", 1)).toBeUndefined();
  });

  // spec: UI-022
  test("filtering by key and free text combines", () => {
    const byRepository: Filter = { ...NO_FILTER, key: { dimension: "repository", key: "/src/be" } };
    expect(groups("none", byRepository)[0]?.[1]).toEqual(["t-checkout", "t-flaky"]);
    expect(groups("none", { ...NO_FILTER, text: "FEAT/check" })[0]?.[1]).toEqual(["t-checkout"]);
    const both: Filter = { text: "ci", key: { dimension: "tag", key: "release" }, archived: false };
    expect(groups("none", both)[0]?.[1]).toEqual(["t-flaky"]);
  });

  // spec: UI-022, UI-026
  test("archived threads are neither listed nor offered as filter keys", () => {
    const { workspaces, threads } = index();
    const archived = threads.map((summary) => (summary.id === "t-flaky" ? { ...summary, archivedAt: 5 } : summary));
    expect(groupThreads(archived, workspaces, "none", NO_FILTER)[0]?.threadIds).toEqual(["t-checkout", "t-scratch"]);
    const tags = filterOptions(archived, workspaces)
      .filter((option) => option.dimension === "tag")
      .map((option) => option.key.key);
    expect(tags).toEqual(["release"]);
  });

  // spec: UI-026
  test("the archived filter lists only archived threads", () => {
    const { workspaces, threads } = index();
    const archived = threads.map((summary) => (summary.id === "t-flaky" ? { ...summary, archivedAt: 5 } : summary));
    const list = (text: string) =>
      groupThreads(archived, workspaces, "none", { text, archived: true })[0]?.threadIds ?? [];
    expect(list("")).toEqual(["t-flaky"]);
    expect(list("checkout")).toEqual([]);
  });

  // spec: UI-021
  test("next needing attention wraps around after the current thread", () => {
    const { threads } = index();
    expect(nextNeedingAttention(threads, undefined)).toBeUndefined();
    const waiting = threads.map((summary) =>
      summary.id === "t-flaky" ? summary : { ...summary, needsAttention: true },
    );
    expect(waitingCount(waiting)).toBe(2);
    expect(nextNeedingAttention(waiting, undefined)).toBe("t-checkout");
    expect(nextNeedingAttention(waiting, "t-checkout")).toBe("t-scratch");
    expect(nextNeedingAttention(waiting, "t-scratch")).toBe("t-checkout");
    const one = waiting.map((summary) =>
      summary.id === "t-scratch" ? { ...summary, needsAttention: false } : summary,
    );
    expect(nextNeedingAttention(one, "t-checkout")).toBe("t-checkout");
  });

  // spec: UI-020
  test("a thread is unread when it changed since it was last seen and is not selected", () => {
    const summary = thread("t", "t", "w", { updatedAt: 10 });
    expect(isUnread(summary, new Map(), undefined)).toBe(false);
    const seen = new Map([["t", 10]]);
    expect(isUnread(summary, seen, undefined)).toBe(false);
    expect(isUnread({ ...summary, updatedAt: 11 }, seen, undefined)).toBe(true);
    expect(isUnread({ ...summary, updatedAt: 11 }, seen, "t")).toBe(false);
  });

  // spec: UI-020
  test("the index sets unread baselines and the selected thread stays seen", () => {
    const first = thread("t1", "one", "w", { updatedAt: 5 });
    const second = thread("t2", "two", "w", { updatedAt: 7 });
    const seen = updateSeen(new Map(), [first, second], undefined);
    expect([...seen]).toEqual([
      ["t1", 5],
      ["t2", 7],
    ]);
    expect(updateSeen(seen, [first, second], undefined)).toBe(seen);
    const changed = [
      { ...first, updatedAt: 9 },
      { ...second, updatedAt: 9 },
    ];
    const selected = updateSeen(seen, changed, "t1");
    expect([...selected]).toEqual([
      ["t1", 9],
      ["t2", 7],
    ]);
  });
});
