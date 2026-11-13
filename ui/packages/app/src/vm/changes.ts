/**
 * The `omfp:changes` widget (UI-080): for each repository of the current workspace (the workspace folder's own marked,
 * UI-341), its branch, ahead/behind and
 * changed files with +/− counts (the `workspace/<id>` topic's git status, DMN-026), and the key that changes when the
 * selected thread's agent may have changed files.
 */

import type { ToolRun } from "@omfp/core";
import type { FileStatus, RepoStatus, Workspace } from "@omfp/protocol";
import { strings } from "../strings/index.ts";
import { gitSummary } from "./workspaces.ts";

const text = strings.shell.workspace;

/** Tools whose end refreshes the changes (UI-080). */
const CHANGE_TOOLS: Readonly<Record<string, true>> = { edit: true, write: true, bash: true };

/** One changed file. */
export interface ChangedFile {
  /** Path in the repository. */
  readonly path: string;
  /** Previous path of a renamed file. */
  readonly from: string | undefined;
  readonly status: FileStatus;
  readonly additions: number;
  readonly deletions: number;
}

/** One repository of the workspace. */
export interface ChangesRepo {
  readonly name: string;
  /** The repository is the workspace folder itself, listed first and marked as the workspace (UI-341). */
  readonly isWorkspace: boolean;
  /** Checked-out branch (live status first, else the index), or `detached`. */
  readonly branch: string;
  /** `↑2 ↓1` when HEAD and its upstream differ. */
  readonly aheadBehind: string | undefined;
  /** `3 files +10 −1`, or `clean`; `undefined` until the status is known. */
  readonly summary: string | undefined;
  /** `undefined` until the status is known. */
  readonly files: readonly ChangedFile[] | undefined;
}

/** The repositories of `workspace` with their changes; `status` is the `workspace/<id>` topic's state. */
export function changesView(workspace: Workspace, status: readonly RepoStatus[] | undefined): ChangesRepo[] {
  return workspace.repos.map((repo) => {
    const git = status?.find((entry) => entry.name === repo.name)?.status;
    if (git === undefined) {
      return {
        name: repo.name,
        isWorkspace: repo.isRoot === true,
        branch: repo.branch ?? text.detached,
        aheadBehind: undefined,
        summary: undefined,
        files: undefined,
      };
    }
    return {
      name: repo.name,
      isWorkspace: repo.isRoot === true,
      branch: git.branch ?? text.detached,
      aheadBehind: git.ahead > 0 || git.behind > 0 ? text.aheadBehind(git.ahead, git.behind) : undefined,
      summary: gitSummary({ ...git, ahead: 0, behind: 0, dirty: false }),
      files: git.files.map((file) => ({
        path: file.path,
        from: file.oldPath ?? undefined,
        status: file.status,
        additions: file.additions,
        deletions: file.deletions,
      })),
    };
  });
}

/** The ids of the thread's ended `edit`, `write` and `bash` runs: it changes each time one of them ends. */
export function changeRunsKey(tools: readonly ToolRun[] | undefined): string {
  return (tools ?? [])
    .filter(
      (run) => Object.hasOwn(CHANGE_TOOLS, run.tool_name) && (run.status === "succeeded" || run.status === "failed"),
    )
    .map((run) => run.tool_call_id)
    .join(" ");
}
