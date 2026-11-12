/** L1: the Workspace widget's view-model (port of the Dioxus `vm/workspaces.rs` tests). */

import { describe, expect, test } from "bun:test";
import type { GitStatus, WorkspaceRepo } from "@omfp/protocol";
import {
  ADD_KINDS,
  addRepoSource,
  currentWorkspaceId,
  gitSummary,
  removalNeedsConfirmation,
  workspaceRow,
} from "../../src/vm/workspaces.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";

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
  test("a row lists repositories with kind, origin, branch and git status, and flags shared workspaces", () => {
    const linked: WorkspaceRepo = { ...repo("w1", "be", "main"), kind: "link", target: "/src/be" };
    const cloned: WorkspaceRepo = { ...repo("w1", "api", null), kind: "clone" };
    const workspaces = [workspace("w1", [repo("w1", "fe", "old"), linked, cloned]), workspace("w2")];
    const threads = [thread("t1", "one", "w1"), thread("t2", "", "w1"), thread("t3", "old", "w2", { archivedAt: 3 })];
    const [first, second] = workspaces;
    if (first === undefined || second === undefined) {
      throw new Error("fixture");
    }
    const row = workspaceRow(first, threads, workspaces, [{ name: "fe", status }]);
    expect([row.id, row.name, row.path, row.threadCount, row.shared]).toEqual(["w1", "w1", "/work/w1", 2, true]);
    expect(row.repos).toEqual([
      {
        name: "fe",
        kind: "worktree",
        branch: "feat",
        origin: "of /src/fe",
        git: "↑2 ↓0 · dirty · 2 files +10 −1",
        dirty: true,
        alsoIn: [],
      },
      { name: "be", kind: "link", branch: "main", origin: "→ /src/be", git: undefined, dirty: undefined, alsoIn: [] },
      {
        name: "api",
        kind: "clone",
        branch: "detached",
        origin: undefined,
        git: undefined,
        dirty: undefined,
        alsoIn: [],
      },
    ]);
    const empty = workspaceRow(second, threads, workspaces, undefined);
    expect([empty.threadCount, empty.shared, empty.repos]).toEqual([0, false, []]);
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
  test("the git summary says clean and ahead/behind", () => {
    expect(gitSummary({ ahead: 0, behind: 3, dirty: false, files: [] })).toBe("↑0 ↓3 · clean");
    expect(gitSummary({ branch: "main", ahead: 0, behind: 0, dirty: true, files: [] })).toBe("dirty");
  });

  // spec: UI-025, UI-100
  test("the window's workspace is the selected thread's, else the draft's; none while the thread is unknown", () => {
    const t1 = thread("t1", "one", "w1");
    expect(currentWorkspaceId("t1", t1, { workspaceId: "w2" })).toBe("w1");
    expect(currentWorkspaceId(undefined, undefined, { workspaceId: "w2" })).toBe("w2");
    expect(currentWorkspaceId(undefined, undefined, undefined)).toBeUndefined();
    // A selected thread not in the index yet (just created): no workspace, not a flash of the draft's.
    expect(currentWorkspaceId("t2", undefined, { workspaceId: "w2" })).toBeUndefined();
  });

  // spec: UI-025
  test("the add control offers link and clone only, built from their trimmed fields", () => {
    expect(ADD_KINDS).toEqual(["link", "clone"]);
    const fields = { path: " /src/fe ", url: "" };
    expect(addRepoSource("link", fields)).toEqual({ kind: "link", path: "/src/fe" });
    expect(addRepoSource("clone", fields)).toBeUndefined();
    expect(addRepoSource("clone", { ...fields, url: " https://example.invalid/x.git" })).toEqual({
      kind: "clone",
      url: "https://example.invalid/x.git",
    });
    expect(addRepoSource("link", { path: "  ", url: "" })).toBeUndefined();
  });

  // spec: UI-025
  test("only links are removed without confirmation", () => {
    expect([
      removalNeedsConfirmation("link"),
      removalNeedsConfirmation("worktree"),
      removalNeedsConfirmation("clone"),
    ]).toEqual([false, true, true]);
  });
});
