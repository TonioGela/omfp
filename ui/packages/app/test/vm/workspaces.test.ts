/** L1: the Workspace widget's view-model (port of the Dioxus `vm/workspaces.rs` tests). */

import { describe, expect, test } from "bun:test";
import type { GitStatus, WorkspaceRepo } from "@omfp/protocol";
import {
  gitSummary,
  removalNeedsConfirmation,
  repositoryRows,
  workspaceList,
  workspaceRow,
} from "../../src/vm/workspaces.ts";
import { repo, repository, thread, workspace } from "../support/fixtures.ts";

const status: GitStatus = {
  branch: "feat",
  baseRef: "main",
  ahead: 2,
  behind: 0,
  dirty: true,
  files: [
    { path: "a", oldPath: null, status: "modified", additions: 3, deletions: 1 },
    { path: "b", oldPath: null, status: "added", additions: 7, deletions: 0 },
  ],
};

describe("workspaces", () => {
  // spec: UI-025, UI-011
  test("a row lists living threads, repositories with kind, origin and git status, and flags shared workspaces", () => {
    const linked: WorkspaceRepo = { ...repo("w1", "be", "main"), kind: "link", target: "/src/be" };
    const workspaces = [workspace("w1", [repo("w1", "fe", "feat"), linked]), workspace("w2")];
    const threads = [thread("t1", "one", "w1"), thread("t2", "", "w1"), thread("t3", "old", "w2", { archivedAt: 3 })];
    const [first, second] = workspaces;
    if (first === undefined || second === undefined) {
      throw new Error("fixture");
    }
    const row = workspaceRow(first, threads, workspaces, [{ name: "fe", status }]);
    expect(row.threads).toEqual([
      { id: "t1", title: "one" },
      { id: "t2", title: "New thread" },
    ]);
    expect(row.shared).toBe(true);
    expect(row.owned).toBe(true);
    expect(row.repos).toEqual([
      {
        name: "fe",
        kind: "worktree",
        label: "fe:feat",
        origin: "worktree of /src/fe",
        git: "feat · ↑2 ↓0 · dirty · 2 files +10 −1",
        dirty: true,
        alsoIn: [],
      },
      { name: "be", kind: "link", label: "be:main", origin: "→ /src/be", git: undefined, dirty: undefined, alsoIn: [] },
    ]);
    const empty = workspaceRow(second, threads, workspaces, undefined);
    expect([empty.threads, empty.shared, empty.repos]).toEqual([[], false, []]);
  });

  // spec: UI-025
  test("a checkout also in another workspace with live threads gets a badge (DMN-027)", () => {
    const shared: WorkspaceRepo = { ...repo("w1", "fe", "main"), sharedWith: ["w2", "w3"] };
    const workspaces = [workspace("w1", [shared]), workspace("w2"), workspace("w3")];
    const threads = [thread("t2", "two", "w2"), thread("t3", "three", "w3", { archivedAt: 1 })];
    const [first] = workspaces;
    if (first === undefined) {
      throw new Error("fixture");
    }
    expect(workspaceRow(first, threads, workspaces, []).repos[0]?.alsoIn).toEqual(["w2"]);
  });

  // spec: UI-025
  test("the git summary says detached, clean and ahead/behind", () => {
    expect(gitSummary({ ahead: 0, behind: 3, dirty: false, files: [] })).toBe("detached · ↑0 ↓3 · clean");
    expect(gitSummary({ branch: "main", ahead: 0, behind: 0, dirty: true, files: [] })).toBe("main · dirty");
  });

  // spec: UI-025
  test("repository rows count the worktrees of each repository", () => {
    const linked: WorkspaceRepo = { ...repo("w2", "be", "main"), kind: "link", target: "/src/be" };
    const workspaces = [workspace("w1", [repo("w1", "fe", "a")]), workspace("w2", [repo("w2", "fe", "b"), linked])];
    expect(repositoryRows([repository("r-fe", "fe"), repository("r-be", "be")], workspaces)).toEqual([
      { id: "r-fe", name: "fe", path: "/src/fe", defaultBaseRef: "main", worktrees: 2 },
      { id: "r-be", name: "be", path: "/src/be", defaultBaseRef: "main", worktrees: 0 },
    ]);
  });

  // spec: UI-025
  test("the list of all workspaces counts their living threads; only links are removed without confirmation", () => {
    const workspaces = [workspace("w1"), workspace("w2")];
    const threads = [thread("t1", "a", "w1"), thread("t2", "b", "w1", { archivedAt: 2 })];
    expect(workspaceList(workspaces, threads).map((row) => [row.id, row.threads])).toEqual([
      ["w1", 1],
      ["w2", 0],
    ]);
    expect([
      removalNeedsConfirmation("link"),
      removalNeedsConfirmation("worktree"),
      removalNeedsConfirmation("clone"),
    ]).toEqual([false, true, true]);
  });
});
