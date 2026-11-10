/**
 * The Workspace widget (UI-025): a workspace with its threads and its repositories with their git status, a warning
 * when several threads work in it or a checkout is in another workspace with live threads (DMN-027), the list of all
 * workspaces and the known repositories.
 */

import type { GitStatus, RepoStatus, Repository, ThreadSummary, Workspace, WorkspaceRepo } from "@omfp/protocol";
import { strings } from "../strings/index.ts";
import { displayTitle, isArchived, type RepoKind, repoLabel } from "./threads.ts";

const text = strings.shell.workspace;

/** A thread living in a workspace. */
export interface LivingThread {
  readonly id: string;
  readonly title: string;
}

/** One repository of a workspace. */
export interface RepoRow {
  /** Entry name in the folder. */
  readonly name: string;
  readonly kind: RepoKind;
  /** `name:branch`, or `name` on a detached HEAD. */
  readonly label: string;
  /** `→ /src/be` for a link, `worktree of /src/fe` for a worktree, `clone` for a clone. */
  readonly origin: string;
  /** `feat · ↑2 ↓0 · dirty · 2 files +10 −1` once the status is known. */
  readonly git: string | undefined;
  /** Uncommitted changes; `undefined` while the status is unknown. */
  readonly dirty: boolean | undefined;
  /** Names of other workspaces with live threads holding the same checkout (DMN-027). */
  readonly alsoIn: readonly string[];
}

/** One workspace. */
export interface WorkspaceRow {
  readonly id: string;
  readonly name: string;
  readonly path: string;
  /** The daemon created the folder. */
  readonly owned: boolean;
  /** Non-archived threads living in it. */
  readonly threads: readonly LivingThread[];
  /** More than one thread works in it (DMN-027). */
  readonly shared: boolean;
  readonly repos: readonly RepoRow[];
}

/** The row of `workspace`; `status` is the `workspace/<id>` topic's git status (`undefined` until it arrives). */
export function workspaceRow(
  workspace: Workspace,
  threads: readonly ThreadSummary[],
  workspaces: readonly Workspace[],
  status: readonly RepoStatus[] | undefined,
): WorkspaceRow {
  const living = livingThreads(workspace.id, threads);
  return {
    id: workspace.id,
    name: workspace.name,
    path: workspace.path,
    owned: workspace.owned,
    threads: living,
    shared: living.length > 1,
    repos: workspace.repos.map((repo) => {
      const git = status?.find((entry) => entry.name === repo.name)?.status;
      return {
        name: repo.name,
        kind: repo.kind,
        label: repoLabel(repo),
        origin: repoOrigin(repo),
        git: git === undefined ? undefined : gitSummary(git),
        dirty: git?.dirty,
        alsoIn: sharedWithLive(repo, workspaces, threads),
      };
    }),
  };
}

function livingThreads(workspaceId: string, threads: readonly ThreadSummary[]): LivingThread[] {
  return threads
    .filter((thread) => !isArchived(thread) && thread.workspaceId === workspaceId)
    .map((thread) => ({ id: thread.id, title: displayTitle(thread) }));
}

/** Where a repository comes from: a link's target, a worktree's main repository, or a clone. */
export function repoOrigin(repo: WorkspaceRepo): string {
  switch (repo.kind) {
    case "link":
      return text.linkTarget(repo.target);
    case "worktree":
      return text.worktreeOf(repo.main);
    case "clone":
      return text.clone;
  }
}

/** The names of the workspaces in `repo.sharedWith` that have live (non-archived) threads (DMN-027). */
export function sharedWithLive(
  repo: WorkspaceRepo,
  workspaces: readonly Workspace[],
  threads: readonly ThreadSummary[],
): string[] {
  return workspaces
    .filter((workspace) => repo.sharedWith.includes(workspace.id) && livingThreads(workspace.id, threads).length > 0)
    .map((workspace) => workspace.name);
}

/** One workspace in the list of all workspaces. */
export interface WorkspaceListRow {
  readonly id: string;
  readonly name: string;
  readonly path: string;
  /** Non-archived threads living in it. */
  readonly threads: number;
}

export function workspaceList(workspaces: readonly Workspace[], threads: readonly ThreadSummary[]): WorkspaceListRow[] {
  return workspaces.map((workspace) => ({
    id: workspace.id,
    name: workspace.name,
    path: workspace.path,
    threads: livingThreads(workspace.id, threads).length,
  }));
}

/** One known repository. */
export interface RepositoryRow {
  readonly id: string;
  readonly name: string;
  readonly path: string;
  readonly defaultBaseRef: string;
  /** Worktrees of it in workspaces. */
  readonly worktrees: number;
}

/** Repository rows in index order. */
export function repositoryRows(repositories: readonly Repository[], workspaces: readonly Workspace[]): RepositoryRow[] {
  const repos = workspaces.flatMap((workspace) => workspace.repos);
  return repositories.map((repository) => ({
    id: repository.id,
    name: repository.name,
    path: repository.path,
    defaultBaseRef: repository.defaultBaseRef,
    worktrees: repos.filter((repo) => repo.kind === "worktree" && repo.main === repository.path).length,
  }));
}

/** One-line git summary: branch, ahead/behind, dirty, changed files. */
export function gitSummary(status: GitStatus): string {
  const parts = [status.branch ?? text.detached];
  if (status.ahead > 0 || status.behind > 0) {
    parts.push(text.aheadBehind(status.ahead, status.behind));
  }
  if (status.dirty) {
    parts.push(text.dirty);
  }
  if (status.files.length > 0) {
    const additions = status.files.reduce((sum, file) => sum + file.additions, 0);
    const deletions = status.files.reduce((sum, file) => sum + file.deletions, 0);
    parts.push(text.files(status.files.length, additions, deletions));
  } else if (!status.dirty) {
    parts.push(text.clean);
  }
  return parts.join(" · ");
}

/** Removing a link only deletes the symlink; anything else asks first (UI-025, DMN-025). */
export function removalNeedsConfirmation(kind: RepoKind): boolean {
  return kind !== "link";
}
