/** L0: the view model of `omfp:changes` (UI-080): changed files per repository and when to refresh. */

import { describe, expect, test } from "bun:test";
import type { GitStatus, WorkspaceRepo } from "@omfp/protocol";
import { changeRunsKey, changesView } from "../../src/vm/changes.ts";
import { repo, workspace } from "../support/fixtures.ts";
import { fixture, fixtureUntil, isFrame } from "../widgets/chat/fixtures.ts";

const status: GitStatus = {
  branch: "feat",
  baseRef: "origin/feat",
  ahead: 2,
  behind: 1,
  dirty: true,
  files: [
    { path: "src/lib.rs", oldPath: null, status: "modified", additions: 3, deletions: 1 },
    { path: "docs/new.md", oldPath: "docs/old.md", status: "renamed", additions: 0, deletions: 0 },
    { path: "notes.txt", oldPath: null, status: "untracked", additions: 7, deletions: 0 },
  ],
};

describe("changes", () => {
  // spec: UI-080, UI-011
  test("each repository lists its changed files with +/− counts, totals, branch and ahead/behind", () => {
    const linked: WorkspaceRepo = { ...repo("w1", "be", "main"), kind: "link", target: "/src/be" };
    const view = changesView(workspace("w1", [repo("w1", "fe", "old"), linked]), [{ name: "fe", status }]);
    expect(view).toEqual([
      {
        name: "fe",
        branch: "feat",
        aheadBehind: "↑2 ↓1",
        summary: "3 files +10 −1",
        files: [
          { path: "src/lib.rs", from: undefined, status: "modified", additions: 3, deletions: 1 },
          { path: "docs/new.md", from: "docs/old.md", status: "renamed", additions: 0, deletions: 0 },
          { path: "notes.txt", from: undefined, status: "untracked", additions: 7, deletions: 0 },
        ],
      },
      // No status yet: the files are unknown, not empty.
      { name: "be", branch: "main", aheadBehind: undefined, summary: undefined, files: undefined },
    ]);
  });

  // spec: UI-080
  test("a clean repository says so; a detached HEAD has no branch", () => {
    const clean: GitStatus = { branch: null, ahead: 0, behind: 0, dirty: false, files: [] };
    const [row] = changesView(workspace("w1", [repo("w1", "fe", null)]), [{ name: "fe", status: clean }]);
    expect([row?.branch, row?.aheadBehind, row?.summary, row?.files]).toEqual(["detached", undefined, "clean", []]);
  });

  // spec: UI-080
  test("the refresh key changes when an edit, write or bash run ends, not while it runs", () => {
    const running = fixtureUntil("bash_tool_call", isFrame("tool_execution_end"));
    expect(running.tools.map((run) => run.status)).toEqual(["running"]);
    expect(changeRunsKey(running.tools)).toBe("");
    expect(changeRunsKey(fixture("bash_tool_call").tools)).toBe("call_bash_1");
    expect(changeRunsKey(undefined)).toBe("");
    // Reads never refresh the changes; failed edits do (a partial edit may have happened).
    expect(changeRunsKey(fixture("read_tool_calls").tools)).toBe("");
    expect(changeRunsKey(fixture("edit_tool_calls").tools)).toBe("call_edit_1 call_edit_2");
    expect(changeRunsKey(fixture("write_tool_calls").tools)).toBe("call_write_1 call_write_2");
  });
});
