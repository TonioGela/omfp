/**
 * Where the nodes of a `views.json` view come from once init files changed it (UI-269, experimental, branch
 * idea/init): each node of the shown layout is matched to the node it was before each file's changes, down to
 * `views.json`'s layout, so Edit layout writes the nodes `views.json` holds (the init files then change the new file
 * again) and leaves alone the ones an init file added.
 */

import { strings } from "../strings/index.ts";
import type { LayoutEdit } from "./edit.ts";
import type { LayoutTree, Parsed } from "./layout.ts";

type InitFile = "user" | "workspace";

const text = strings.init.edit;

/** A shown node: the node at `base` in `views.json`'s layout, or one the init file `init` added. */
export type NodeOrigin = { readonly base: string } | { readonly init: InitFile };

/** The nodes of a `views.json` view init files changed. */
export interface NodeOrigins {
  /** The view's layout in `views.json`: what Edit layout writes. */
  readonly base: LayoutTree;
  /** By path in the shown layout (`0`, `0.1`, … as the renderer numbers its nodes). */
  readonly nodes: ReadonlyMap<string, NodeOrigin>;
}

/** The layout after one init file's changes. */
export interface LayoutLayer {
  readonly file: InitFile;
  readonly tree: LayoutTree;
}

/** Where each node of the last layer comes from: `base` through every layer, or the file of the layer that added it. */
export function nodeOrigins(base: LayoutTree, layers: readonly LayoutLayer[]): NodeOrigins {
  let nodes = new Map<string, NodeOrigin>(flatten(base, "0").map(([path]) => [path, { base: path }]));
  let previous = base;
  for (const { file, tree } of layers) {
    const matched = match(previous, tree);
    const next = new Map<string, NodeOrigin>();
    for (const [path] of flatten(tree, "0")) {
      const before = matched.get(path);
      next.set(path, (before === undefined ? undefined : nodes.get(before)) ?? { init: file });
    }
    nodes = next;
    previous = tree;
  }
  return { base, nodes };
}

/** The init file that added the node at `path`; `undefined` for a node of `views.json` (or without origins). */
export function addedBy(origins: NodeOrigins | undefined, path: string): InitFile | undefined {
  const origin = origins?.nodes.get(path);
  return origin !== undefined && "init" in origin ? origin.init : undefined;
}

/**
 * `edit`, written with paths of the shown layout, on `views.json`'s layout: a node an init file added cannot be moved,
 * hidden or be a Move target, and a splitter resizes `views.json`'s sizes only between cells all from `views.json`.
 */
export function toBase(origins: NodeOrigins, shown: LayoutTree, edit: LayoutEdit): Parsed<LayoutEdit> {
  switch (edit.kind) {
    case "add":
      return { ok: true, value: edit };
    case "hide": {
      const path = basePath(origins, edit.path);
      return path.ok ? { ok: true, value: { ...edit, path: path.value } } : path;
    }
    case "move": {
      const from = basePath(origins, edit.from);
      if (!from.ok) {
        return from;
      }
      const to = basePath(origins, edit.to);
      return to.ok ? { ok: true, value: { ...edit, from: from.value, to: to.value } } : to;
    }
    case "resize": {
      const path = basePath(origins, edit.path);
      if (!path.ok) {
        return path;
      }
      const cells = at(shown, edit.path);
      const written = at(origins.base, path.value);
      if (cells === undefined || cells.kind === "widget" || written === undefined || written.kind === "widget") {
        return { ok: false, error: strings.shell.layout.notContainer(edit.path) };
      }
      const added = cells.children.map((_, index) => addedBy(origins, `${edit.path}.${index}`)).find(Boolean);
      if (added !== undefined) {
        return { ok: false, error: text.added(strings.init.files[added]) };
      }
      const same =
        cells.children.length === written.children.length &&
        cells.children.every((_, index) => {
          const origin = origins.nodes.get(`${edit.path}.${index}`);
          return origin !== undefined && "base" in origin && origin.base === `${path.value}.${index}`;
        });
      return same ? { ok: true, value: { ...edit, path: path.value } } : { ok: false, error: text.cells };
    }
  }
}

function basePath(origins: NodeOrigins, path: string): Parsed<string> {
  const origin = origins.nodes.get(path);
  if (origin === undefined) {
    return { ok: false, error: strings.shell.layout.noNode(path) };
  }
  return "base" in origin
    ? { ok: true, value: origin.base }
    : { ok: false, error: text.added(strings.init.files[origin.init]) };
}

/** Every node of `tree` with its path, in tree order (the root first). */
function flatten(tree: LayoutTree, path: string): [string, LayoutTree][] {
  return tree.kind === "widget"
    ? [[path, tree]]
    : [[path, tree], ...tree.children.flatMap((child, index) => flatten(child, `${path}.${index}`))];
}

function at(tree: LayoutTree, path: string): LayoutTree | undefined {
  return flatten(tree, "0").find(([candidate]) => candidate === path)?.[1];
}

/** What makes two nodes the same across a change: a widget's reference, a container's kind and `id`. */
function signature(tree: LayoutTree): string {
  return tree.kind === "widget"
    ? `widget ${tree.name}`
    : `${tree.kind} ${typeof tree.props.id === "string" ? tree.props.id : ""}`;
}

/**
 * Path in `next` → path in `previous` of the nodes `next` kept: the roots (or `previous`'s root wrapped in new
 * containers, or a node of `previous` that became the root) when they have the same signature, then their children
 * matched in order (the longest common subsequence of signatures), recursively.
 */
function match(previous: LayoutTree, next: LayoutTree): Map<string, string> {
  const matched = new Map<string, string>();
  const wrapped = flatten(next, "0").find(([, tree]) => signature(tree) === signature(previous));
  if (wrapped !== undefined) {
    pair(previous, "0", wrapped[1], wrapped[0], matched);
    return matched;
  }
  const unwrapped = flatten(previous, "0").find(([, tree]) => signature(tree) === signature(next));
  if (unwrapped !== undefined) {
    pair(unwrapped[1], unwrapped[0], next, "0", matched);
  }
  return matched;
}

function pair(
  previous: LayoutTree,
  previousPath: string,
  next: LayoutTree,
  nextPath: string,
  matched: Map<string, string>,
): void {
  matched.set(nextPath, previousPath);
  if (previous.kind === "widget" || next.kind === "widget") {
    return;
  }
  const before = previous.children.map(signature);
  const after = next.children.map(signature);
  // lengths[i * width + j]: the longest common subsequence of before[i..] and after[j..].
  const width = after.length + 1;
  const lengths = new Uint16Array((before.length + 1) * width);
  const length = (i: number, j: number) => lengths[i * width + j] ?? 0;
  for (let i = before.length - 1; i >= 0; i -= 1) {
    for (let j = after.length - 1; j >= 0; j -= 1) {
      lengths[i * width + j] =
        before[i] === after[j] ? length(i + 1, j + 1) + 1 : Math.max(length(i + 1, j), length(i, j + 1));
    }
  }
  let i = 0;
  let j = 0;
  while (i < before.length && j < after.length) {
    const from = previous.children[i];
    const to = next.children[j];
    if (before[i] === after[j] && from !== undefined && to !== undefined) {
      pair(from, `${previousPath}.${i}`, to, `${nextPath}.${j}`, matched);
      i += 1;
      j += 1;
    } else if (length(i + 1, j) >= length(i, j + 1)) {
      i += 1;
    } else {
      j += 1;
    }
  }
}
