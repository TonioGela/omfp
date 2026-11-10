/**
 * The thread list (UI-020..022, UI-026): state badges, workspace chips, grouping, filtering, unread marks and the "next
 * thread needing attention" shortcut. Workspaces and repositories are groupings, never parents.
 */

import type { ThreadSummary, Workspace, WorkspaceRepo } from "@omfp/protocol";
import { strings } from "../strings/index.ts";

/** What a thread's badge says; also its `data-status` token. */
export type ThreadStatus = "awaiting" | "failed" | "running" | "starting" | "idle" | "stopped" | "terminal";

/** Attention first, then the process, then omp's settled flag. */
export function threadStatus(thread: ThreadSummary): ThreadStatus {
  if (thread.needsAttention) {
    return "awaiting";
  }
  switch (thread.process.state) {
    case "failed":
      return "failed";
    case "starting":
      return "starting";
    case "terminal":
      return "terminal";
    case "running":
      return thread.settled ? "idle" : "running";
    case "stopped":
      return "stopped";
  }
}

export function statusLabel(status: ThreadStatus): string {
  return strings.shell.status[status];
}

const GLYPHS: Readonly<Record<ThreadStatus, string>> = {
  awaiting: "!",
  failed: "✕",
  running: "◐",
  starting: "◐",
  idle: "●",
  stopped: "○",
  terminal: "▣",
};

/** One character shown before the title (the token and label carry the meaning). */
export function statusGlyph(status: ThreadStatus): string {
  return GLYPHS[status];
}

/** The title to show: the thread's, or "New thread" while it has none (DMN-073). */
export function displayTitle(thread: Pick<ThreadSummary, "title">): string {
  return thread.title.trim() === "" ? strings.shell.untitled : thread.title;
}

export type RepoKind = WorkspaceRepo["kind"];

/** A thread's workspace, or one of its repositories, as shown next to the thread. */
export interface WorkspaceChip {
  /** Unique among a thread's chips: the workspace id, then `<workspace id>/<repository name>`. */
  readonly key: string;
  /** `workspace` (the thread's cwd), or the repository's kind. */
  readonly kind: "workspace" | RepoKind;
  /** The workspace's name, or the repository's `name:branch` (`name` on a detached HEAD). */
  readonly label: string;
}

/** The chip of the workspace, then one per repository in folder order; none for an unknown id. */
export function workspaceChips(workspaceId: string, workspaces: readonly Workspace[]): WorkspaceChip[] {
  const workspace = workspaces.find((candidate) => candidate.id === workspaceId);
  if (workspace === undefined) {
    return [];
  }
  return [
    { key: workspace.id, kind: "workspace", label: workspace.name },
    ...workspace.repos.map((repo) => ({
      key: `${workspace.id}/${repo.name}`,
      kind: repo.kind,
      label: repoLabel(repo),
    })),
  ];
}

/** `name:branch`, or `name` on a detached HEAD. */
export function repoLabel(repo: Pick<WorkspaceRepo, "name" | "branch">): string {
  return repo.branch === undefined || repo.branch === null ? repo.name : `${repo.name}:${repo.branch}`;
}

/** What makes repositories of different workspaces the same one: a worktree's main working tree, else the checkout. */
export function repoIdentity(repo: WorkspaceRepo): string {
  return repo.kind === "worktree" ? repo.main : repo.canonicalPath;
}

/** A key threads can be grouped and filtered by (UI-022). Pull requests come with S-08. */
export type Dimension = "tag" | "workspace" | "repository";

export const DIMENSIONS: readonly Dimension[] = ["tag", "workspace", "repository"];

/** How the list groups threads; by workspace by default (UI-022). */
export type Grouping = "none" | Dimension;

export const DEFAULT_GROUPING: Grouping = "workspace";

export function isGrouping(value: unknown): value is Grouping {
  return value === "none" || DIMENSIONS.includes(value as Dimension);
}

/** Which threads the list shows. */
export interface Filter {
  /** Case-insensitive match on title, tags and workspace chip labels. */
  readonly text: string;
  /** Only threads with this key. */
  readonly key?: { readonly dimension: Dimension; readonly key: string };
  /** Only archived threads; they are hidden otherwise (UI-026). */
  readonly archived: boolean;
}

export const NO_FILTER: Filter = { text: "", archived: false };

/** One grouping or filter key of a thread. */
export interface GroupKey {
  readonly key: string;
  readonly label: string;
}

/** The keys of `thread` along `dimension`, without duplicates, in folder order (tags in tag order). */
export function threadKeys(thread: ThreadSummary, dimension: Dimension, workspaces: readonly Workspace[]): GroupKey[] {
  const workspace = workspaces.find((candidate) => candidate.id === thread.workspaceId);
  const keys = ((): GroupKey[] => {
    switch (dimension) {
      case "tag":
        return thread.tags.map((tag) => ({ key: tag, label: tag }));
      case "workspace":
        return workspace === undefined ? [] : [{ key: workspace.id, label: workspace.name }];
      case "repository":
        return (workspace?.repos ?? []).map((repo) => ({ key: repoIdentity(repo), label: repo.name }));
    }
  })();
  const unique: GroupKey[] = [];
  for (const key of keys) {
    if (!unique.some((seen) => seen.key === key.key)) {
      unique.push(key);
    }
  }
  return unique;
}

/** One entry of the filter selector. */
export interface FilterOption {
  readonly dimension: Dimension;
  readonly key: GroupKey;
}

/** Every key of the non-archived threads along every dimension, sorted by label per dimension. */
export function filterOptions(threads: readonly ThreadSummary[], workspaces: readonly Workspace[]): FilterOption[] {
  const options: FilterOption[] = [];
  for (const dimension of DIMENSIONS) {
    const keys = threads
      .filter((thread) => !isArchived(thread))
      .flatMap((thread) => threadKeys(thread, dimension, workspaces))
      .sort((a, b) => compare(a.label, b.label) || compare(a.key, b.key));
    const unique = keys.filter((key, index) => index === 0 || keys[index - 1]?.key !== key.key);
    options.push(...unique.map((key) => ({ dimension, key })));
  }
  return options;
}

/** One section of the thread list. */
export interface Group {
  /** Empty for the single group of `none` and for the "without key" group. */
  readonly key: string;
  readonly label: string;
  readonly threadIds: readonly string[];
}

/**
 * The list's sections: threads that pass `filter` (archived ones only under `filter.archived`, UI-026), in index order
 * within each group. With a grouping, a thread appears under every group it has a key for; threads without any key
 * come last in their own group.
 */
export function groupThreads(
  threads: readonly ThreadSummary[],
  workspaces: readonly Workspace[],
  grouping: Grouping,
  filter: Filter,
): Group[] {
  const visible = threads.filter(
    (thread) => isArchived(thread) === filter.archived && matches(thread, filter, workspaces),
  );
  if (grouping === "none") {
    return [{ key: "", label: strings.shell.threads.allThreads, threadIds: visible.map((thread) => thread.id) }];
  }
  const groups = new Map<string, { label: string; threadIds: string[] }>();
  const without: string[] = [];
  for (const thread of visible) {
    const keys = threadKeys(thread, grouping, workspaces);
    if (keys.length === 0) {
      without.push(thread.id);
    }
    for (const key of keys) {
      const group = groups.get(key.key);
      if (group === undefined) {
        groups.set(key.key, { label: key.label, threadIds: [thread.id] });
      } else {
        group.threadIds.push(thread.id);
      }
    }
  }
  const sorted: Group[] = [...groups.entries()]
    .map(([key, group]) => ({ key, ...group }))
    .sort((a, b) => compare(a.label, b.label));
  if (without.length > 0) {
    sorted.push({ key: "", label: strings.shell.none[grouping], threadIds: without });
  }
  return sorted;
}

/** A move through the list: by `delta` rows (clamped at both ends), or to one end. */
export type Move = number | "top" | "bottom";

/** The thread `move` lands on from `current` in `order`; from no selection, a forward move starts at the top and a
 * backward one at the bottom. */
export function moveSelection(order: readonly string[], current: string | undefined, move: Move): string | undefined {
  if (order.length === 0) {
    return undefined;
  }
  const last = order.length - 1;
  const position = current === undefined ? -1 : order.indexOf(current);
  let target: number;
  if (move === "top") {
    target = 0;
  } else if (move === "bottom") {
    target = last;
  } else if (position < 0) {
    target = move > 0 ? 0 : last;
  } else {
    target = Math.min(last, Math.max(0, position + move));
  }
  return order[target];
}

/** Thread ids in list order, each once (a thread under several groups counts at its first place). */
export function listOrder(groups: readonly Group[]): string[] {
  return [...new Set(groups.flatMap((group) => group.threadIds))];
}

function matches(thread: ThreadSummary, filter: Filter, workspaces: readonly Workspace[]): boolean {
  const wanted = filter.key;
  if (
    wanted !== undefined &&
    !threadKeys(thread, wanted.dimension, workspaces).some((candidate) => candidate.key === wanted.key)
  ) {
    return false;
  }
  const needle = filter.text.trim().toLowerCase();
  if (needle === "") {
    return true;
  }
  const chips = workspaceChips(thread.workspaceId, workspaces).map((chip) => chip.label);
  return [displayTitle(thread), ...thread.tags, ...chips].some((text) => text.toLowerCase().includes(needle));
}

/**
 * The next thread awaiting input after `current` in index order, wrapping around; `current` itself only when it is the
 * only one (UI-021).
 */
export function nextNeedingAttention(
  threads: readonly ThreadSummary[],
  current: string | undefined,
): string | undefined {
  const candidates = threads.filter((thread) => !isArchived(thread));
  const position = current === undefined ? -1 : candidates.findIndex((thread) => thread.id === current);
  const start = position + 1;
  for (let offset = 0; offset < candidates.length; offset += 1) {
    const thread = candidates[(start + offset) % candidates.length];
    if (thread?.needsAttention === true) {
      return thread.id;
    }
  }
  return undefined;
}

/** Non-archived threads awaiting input. */
export function waitingCount(threads: readonly ThreadSummary[]): number {
  return threads.filter((thread) => !isArchived(thread) && thread.needsAttention).length;
}

/** `updatedAt` of each thread when this window last showed it (UI-020). */
export type Seen = ReadonlyMap<string, number>;

/**
 * A thread has news the user has not seen: it changed after the last time it was shown (UI-020). The selected thread
 * is never unread; a thread the window has never observed has no baseline yet and is not unread.
 */
export function isUnread(thread: ThreadSummary, seen: Seen, selected: string | undefined): boolean {
  const baseline = seen.get(thread.id);
  return selected !== thread.id && baseline !== undefined && thread.updatedAt > baseline;
}

/**
 * `seen` after the index changed: threads observed for the first time get their current `updatedAt` as baseline, and
 * the selected thread is always seen. The same object when nothing changes, so callers can skip the write.
 */
export function updateSeen(seen: Seen, threads: readonly ThreadSummary[], selected: string | undefined): Seen {
  const updates = threads.filter((thread) => {
    const baseline = seen.get(thread.id);
    return baseline === undefined || (thread.id === selected && baseline < thread.updatedAt);
  });
  if (updates.length === 0) {
    return seen;
  }
  const next = new Map(seen);
  for (const thread of updates) {
    next.set(thread.id, thread.updatedAt);
  }
  return next;
}

export function isArchived(thread: ThreadSummary): boolean {
  return thread.archivedAt !== undefined && thread.archivedAt !== null;
}

function compare(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}
