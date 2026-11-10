/**
 * The left column (UI-020..023, UI-026): state badges, workspace chips, one section per workspace with its threads and
 * its archived ones, the full-text filter (local fields plus `threads.search`), collapsed sections, unread marks and
 * the "next thread needing attention" shortcut.
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

/** The text filter of the column (UI-022): the query and what `threads.search` found for it. */
export interface ThreadFilter {
  readonly query: string;
  /** Snippet by thread id of the conversation matches of `query` (DMN-076); empty while unknown. */
  readonly conversation: ReadonlyMap<string, string>;
}

export const NO_FILTER: ThreadFilter = { query: "", conversation: new Map() };

/** A query is typed: sections without matching threads are hidden and collapsed ones are shown. */
export function isFiltering(filter: ThreadFilter): boolean {
  return filter.query.trim() !== "";
}

/** `query` (case-insensitive) is in the title, a tag, the workspace's name or one of its repositories' names. */
export function matchesLocally(thread: ThreadSummary, workspace: Workspace | undefined, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === "") {
    return true;
  }
  const fields = [
    displayTitle(thread),
    ...thread.tags,
    ...(workspace === undefined
      ? []
      : [workspace.name, ...workspace.repos.flatMap((repo) => [repo.name, repoLabel(repo)])]),
  ];
  return fields.some((field) => field.toLowerCase().includes(needle));
}

/** The thread passes `filter`: locally or through its conversation. */
export function matchesFilter(thread: ThreadSummary, workspace: Workspace | undefined, filter: ThreadFilter): boolean {
  return matchesLocally(thread, workspace, filter.query) || (isFiltering(filter) && filter.conversation.has(thread.id));
}

/** One workspace of the column. */
export interface Section {
  readonly workspace: Workspace;
  /** Its non-archived threads that pass the filter, in index order. */
  readonly threadIds: readonly string[];
  /** Its archived threads that pass the filter, in index order (UI-026). */
  readonly archivedIds: readonly string[];
}

/**
 * Every workspace by name with its threads (UI-020); while filtering, only threads that match and only workspaces
 * with one (UI-022). Threads of an unknown workspace are not listed.
 */
export function sidebarSections(
  threads: readonly ThreadSummary[],
  workspaces: readonly Workspace[],
  filter: ThreadFilter,
): Section[] {
  const filtering = isFiltering(filter);
  return [...workspaces]
    .sort((a, b) => compare(a.name.toLowerCase(), b.name.toLowerCase()) || compare(a.id, b.id))
    .map((workspace) => {
      const own = threads.filter(
        (thread) => thread.workspaceId === workspace.id && matchesFilter(thread, workspace, filter),
      );
      return {
        workspace,
        threadIds: own.filter((thread) => !isArchived(thread)).map((thread) => thread.id),
        archivedIds: own.filter(isArchived).map((thread) => thread.id),
      };
    })
    .filter((section) => !filtering || section.threadIds.length + section.archivedIds.length > 0);
}

/** The section's threads are hidden: collapsed by the user, unless a filter is typed. */
export function isCollapsed(workspaceId: string, collapsed: readonly string[], filter: ThreadFilter): boolean {
  return !isFiltering(filter) && collapsed.includes(workspaceId);
}

/** The thread rows shown, top to bottom: the threads of open sections, then their archived ones when listed. */
export function visibleOrder(
  sections: readonly Section[],
  collapsed: readonly string[],
  archivedOpen: readonly string[],
  filter: ThreadFilter,
): string[] {
  return sections.flatMap((section) => {
    const id = section.workspace.id;
    if (isCollapsed(id, collapsed, filter)) {
      return [];
    }
    return archivedOpen.includes(id) ? [...section.threadIds, ...section.archivedIds] : [...section.threadIds];
  });
}

/** `ids` with `id` added, or removed when it was there. */
export function toggled(ids: readonly string[], id: string): readonly string[] {
  return ids.includes(id) ? ids.filter((candidate) => candidate !== id) : [...ids, id];
}

/** A stored list of ids (the collapsed sections, kept across reloads); `undefined` when it is not one. */
export function decodeIds(stored: unknown): readonly string[] | undefined {
  return Array.isArray(stored) && stored.every((entry) => typeof entry === "string") ? stored : undefined;
}

/** Queries shorter than this are not sent to `threads.search` (DMN-076). */
export const MIN_SEARCH_LENGTH = 2;
/** Pause after the last keystroke before `threads.search` is called. */
export const SEARCH_DELAY_MS = 200;

/** One `threads.search` hit. */
export interface SearchHit {
  readonly threadId: string;
  readonly snippet: string;
}

/** Runs `run` after `ms` milliseconds; returns the cancel function. */
export type Schedule = (run: () => void, ms: number) => () => void;

/**
 * The conversation part of the filter (UI-022): `threads.search` once typing pauses, reporting only the answer to the
 * latest query (an older answer arriving late is dropped); short queries report no match at once.
 */
export class ConversationSearch {
  readonly #search: (query: string) => Promise<readonly SearchHit[]>;
  readonly #schedule: Schedule;
  readonly #onResults: (query: string, snippets: ReadonlyMap<string, string>) => void;
  #latest = 0;
  #cancel: (() => void) | undefined;

  constructor(
    search: (query: string) => Promise<readonly SearchHit[]>,
    schedule: Schedule,
    onResults: (query: string, snippets: ReadonlyMap<string, string>) => void,
  ) {
    this.#search = search;
    this.#schedule = schedule;
    this.#onResults = onResults;
  }

  /** The query changed. */
  update(query: string): void {
    this.#cancel?.();
    this.#cancel = undefined;
    this.#latest += 1;
    const request = this.#latest;
    const trimmed = query.trim();
    if (trimmed.length < MIN_SEARCH_LENGTH) {
      this.#onResults(query, new Map());
      return;
    }
    const report = (hits: readonly SearchHit[]) => {
      if (request === this.#latest) {
        this.#onResults(query, new Map(hits.map((hit) => [hit.threadId, hit.snippet])));
      }
    };
    this.#cancel = this.#schedule(() => {
      this.#search(trimmed).then(report, () => report([]));
    }, SEARCH_DELAY_MS);
  }

  /** Drops the pending call and any answer still to come. */
  dispose(): void {
    this.#cancel?.();
    this.#cancel = undefined;
    this.#latest += 1;
  }
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
