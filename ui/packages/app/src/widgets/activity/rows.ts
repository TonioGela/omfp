/**
 * The rows of `omfp:activity` (UI-241): the activity log's entries (`activity.list`, PRT-086) as text, newest first,
 * and how newer and older pages join the list.
 */

import type { ActivityEntry, ActivityPage, ThreadSummary, Workspace } from "@omfp/protocol";

export interface ActivityRow {
  readonly seq: number;
  readonly at: number;
  /** `null` for an MCP client's entry (AGT-053). */
  readonly threadId: string | null;
  /** The calling thread's title (its id when the index no longer has it), or `mcp: <client>` for an MCP client. */
  readonly thread: string;
  /** The index still has the calling thread (never for an MCP client). */
  readonly known: boolean;
  readonly method: string;
  /** What the params name: a thread's title, a workspace's name, a path, a command. */
  readonly target: string | undefined;
  readonly error: string | null;
}

/** The entries shown and whether older ones exist. */
export interface ActivityLog {
  readonly entries: readonly ActivityEntry[];
  readonly more: boolean;
}

export function activityRows(
  entries: readonly ActivityEntry[],
  threads: ReadonlyMap<string, ThreadSummary>,
  workspaces: ReadonlyMap<string, Workspace>,
  untitled: string,
  mcpClient: (client: string) => string,
): ActivityRow[] {
  const title = (id: string) => {
    const thread = threads.get(id);
    return thread === undefined ? id : thread.title.trim() === "" ? untitled : thread.title;
  };
  return entries.map((entry) => {
    const threadId = entry.threadId ?? null;
    return {
      seq: entry.seq,
      at: entry.at,
      threadId,
      thread: threadId === null ? mcpClient(entry.client) : title(threadId),
      known: threadId !== null && threads.has(threadId),
      method: entry.method,
      target: target(entry.params, title, workspaces),
      error: entry.error ?? null,
    };
  });
}

function target(
  params: unknown,
  title: (id: string) => string,
  workspaces: ReadonlyMap<string, Workspace>,
): string | undefined {
  if (typeof params !== "object" || params === null) {
    return undefined;
  }
  const fields = params as Readonly<Record<string, unknown>>;
  const workspace = fields.workspace;
  const workspaceId =
    typeof fields.workspaceId === "string"
      ? fields.workspaceId
      : typeof workspace === "object" && workspace !== null && "id" in workspace && typeof workspace.id === "string"
        ? workspace.id
        : undefined;
  const parts = [
    typeof fields.threadId === "string" ? title(fields.threadId) : undefined,
    workspaceId === undefined ? undefined : (workspaces.get(workspaceId)?.name ?? workspaceId),
    typeof fields.path === "string" ? fields.path : undefined,
    typeof fields.command === "string" ? fields.command : undefined,
  ].filter((part) => part !== undefined);
  return parts.length === 0 ? undefined : parts.join(" · ");
}

/** The log after a page of newer entries (`after` its newest): they go on top, or replace it when the page left some
 * out (`more`), so the list never has a gap. Entries already shown are not repeated. */
export function withNewer(log: ActivityLog, page: ActivityPage): ActivityLog {
  if (page.more) {
    return { entries: page.entries, more: true };
  }
  const newest = log.entries[0]?.seq ?? Number.NEGATIVE_INFINITY;
  const fresh = page.entries.filter((entry) => entry.seq > newest);
  return fresh.length === 0 ? log : { entries: [...fresh, ...log.entries], more: log.more };
}

/** The log after a page of older entries (`before` its oldest). */
export function withOlder(log: ActivityLog, page: ActivityPage): ActivityLog {
  const oldest = log.entries.at(-1)?.seq ?? Number.POSITIVE_INFINITY;
  return { entries: [...log.entries, ...page.entries.filter((entry) => entry.seq < oldest)], more: page.more };
}
