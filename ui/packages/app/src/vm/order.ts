/**
 * The order of the left column (UI-620..UI-627): where a dragged thread row or workspace section lands, the moves the
 * keys make, the `threads.reorder` / `workspaces.reorder` params (PRT-170, PRT-171) and what is announced. The order
 * itself is the daemon's (`position`, DMN-284), shared by every window: {@link sidebarSections} sorts by it.
 */

import type { ReorderThreadParams, ReorderWorkspaceParams } from "@omfp/protocol";
import { strings } from "../strings/index.ts";
import type { Section } from "./threads.ts";

const text = strings.shell.reorder;

/** What is dragged or moved: a thread of a workspace, or a workspace section. */
export type ReorderSource =
  | { readonly kind: "thread"; readonly id: string; readonly workspaceId: string }
  | { readonly kind: "workspace"; readonly id: string };

/** Where it lands: just before `beforeId`, or last without one; a thread in `workspaceId`. */
export type ReorderTarget =
  | { readonly kind: "thread"; readonly workspaceId: string; readonly beforeId?: string }
  | { readonly kind: "workspace"; readonly beforeId?: string };

/** `order` with `id` taken out and put back just before `beforeId`, or last (DMN-281, DMN-282). */
export function placed(order: readonly string[], id: string, beforeId: string | undefined): string[] {
  const rest = order.filter((other) => other !== id);
  const at = beforeId === undefined ? -1 : rest.indexOf(beforeId);
  return at === -1 ? [...rest, id] : [...rest.slice(0, at), id, ...rest.slice(at)];
}

/** A section as the pointer sees it: its box and the middles of its thread rows, top to bottom, in client pixels. */
export interface SectionBox {
  readonly workspaceId: string;
  readonly top: number;
  readonly bottom: number;
  readonly rows: ReadonlyArray<{ readonly id: string; readonly middle: number }>;
}

/**
 * Where a dragged thread lands with the pointer at `y` (UI-621): in the section under it (the first one above all of
 * them, the last one below), just before its first row whose middle is below the pointer, else last; a collapsed or
 * empty section takes it last. `undefined` without sections.
 */
export function threadTargetAt(boxes: readonly SectionBox[], y: number): ReorderTarget | undefined {
  const box = boxes.find((candidate) => y < candidate.bottom) ?? boxes.at(-1);
  if (box === undefined) {
    return undefined;
  }
  const before = box.rows.find((row) => row.middle > y)?.id;
  return { kind: "thread", workspaceId: box.workspaceId, ...(before !== undefined && { beforeId: before }) };
}

/** Where a dragged section lands with the pointer at `y` (UI-622): before the first section whose middle is below the
 * pointer, else last. */
export function workspaceTargetAt(boxes: readonly SectionBox[], y: number): ReorderTarget {
  const before = boxes.find((box) => (box.top + box.bottom) / 2 > y)?.workspaceId;
  return { kind: "workspace", ...(before !== undefined && { beforeId: before }) };
}

/** The list `target` puts `source` in, as it is now and as it would be: the section's threads or the sections. */
function lists(sections: readonly Section[], source: ReorderSource, target: ReorderTarget): [string[], string[]] {
  if (source.kind === "workspace" || target.kind === "workspace") {
    const ids = sections.map((section) => section.workspace.id);
    return [ids, placed(ids, source.id, target.beforeId)];
  }
  const ids = [...(sections.find((section) => section.workspace.id === target.workspaceId)?.threadIds ?? [])];
  return [ids, placed(ids, source.id, target.beforeId)];
}

/** Whether `target` leaves everything where it is: dropped back in its place. */
export function isNoop(sections: readonly Section[], source: ReorderSource, target: ReorderTarget): boolean {
  if (source.kind === "thread" && target.kind === "thread" && target.workspaceId !== source.workspaceId) {
    return false;
  }
  const [now, then] = lists(sections, source, target);
  return now.length === then.length && now.every((id, index) => then[index] === id);
}

/** The keys' moves (UI-624): one place up or down, or into the previous or next workspace. */
export type KeyMove = "up" | "down" | "prevWorkspace" | "nextWorkspace";

/**
 * Where `move` takes `source` (UI-624): a thread one place up or down in its workspace, or to the end of the previous
 * workspace or the start of the next one; a workspace one place up or down. `undefined` at an end, and for a
 * workspace moved to another workspace.
 */
export function keyTarget(
  sections: readonly Section[],
  source: ReorderSource,
  move: KeyMove,
): ReorderTarget | undefined {
  const ids = sections.map((section) => section.workspace.id);
  if (source.kind === "workspace") {
    const at = ids.indexOf(source.id);
    if (move === "up" && at > 0) {
      return { kind: "workspace", ...beforeOf(ids[at - 1]) };
    }
    if (move === "down" && at !== -1 && at < ids.length - 1) {
      return { kind: "workspace", ...beforeOf(ids[at + 2]) };
    }
    return undefined;
  }
  const sectionAt = ids.indexOf(source.workspaceId);
  const threads = sections[sectionAt]?.threadIds ?? [];
  const at = threads.indexOf(source.id);
  switch (move) {
    case "up":
      return at > 0 ? { kind: "thread", workspaceId: source.workspaceId, ...beforeOf(threads[at - 1]) } : undefined;
    case "down":
      return at !== -1 && at < threads.length - 1
        ? { kind: "thread", workspaceId: source.workspaceId, ...beforeOf(threads[at + 2]) }
        : undefined;
    case "prevWorkspace": {
      const previous = sections[sectionAt - 1];
      return sectionAt > 0 && previous !== undefined
        ? { kind: "thread", workspaceId: previous.workspace.id }
        : undefined;
    }
    case "nextWorkspace": {
      const next = sections[sectionAt + 1];
      return sectionAt !== -1 && next !== undefined
        ? { kind: "thread", workspaceId: next.workspace.id, ...beforeOf(next.threadIds[0]) }
        : undefined;
    }
  }
}

function beforeOf(id: string | undefined): { beforeId?: string } {
  return id === undefined ? {} : { beforeId: id };
}

/** The method and params of a move (PRT-170, PRT-171): a thread names its workspace only when it changes. */
export function reorderCall(
  source: ReorderSource,
  target: ReorderTarget,
):
  | { readonly method: "threads.reorder"; readonly params: ReorderThreadParams }
  | { readonly method: "workspaces.reorder"; readonly params: ReorderWorkspaceParams } {
  const before = beforeOf(target.beforeId);
  if (source.kind === "workspace") {
    return { method: "workspaces.reorder", params: { workspaceId: source.id, ...before } };
  }
  const moving = target.kind === "thread" && target.workspaceId !== source.workspaceId;
  return {
    method: "threads.reorder",
    params: { threadId: source.id, ...(moving && { workspaceId: target.workspaceId }), ...before },
  };
}

/**
 * What the live region says once a move is done (UI-625): the moved thread's or workspace's `name`, its new place from
 * 1 and the count of its list, and the workspace a thread is in.
 */
export function announcement(
  sections: readonly Section[],
  source: ReorderSource,
  target: ReorderTarget,
  name: string,
): string {
  const [now, then] = lists(sections, source, target);
  const place = then.indexOf(source.id) + 1;
  const count = then.length;
  if (source.kind === "workspace" || target.kind === "workspace") {
    return text.movedWorkspace(name, place, count);
  }
  const workspace = sections.find((section) => section.workspace.id === target.workspaceId)?.workspace.name ?? "";
  return now.includes(source.id)
    ? text.movedThread(name, place, count, workspace)
    : text.movedThreadTo(name, workspace, place, count);
}
