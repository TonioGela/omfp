/**
 * Layout edits (UI-103, UI-104): pure functions over a view's layout as written in `views.json`, so the frame writes
 * the result with `views.set` and renders what the daemon pushes back. Nodes are addressed by path: `"0"` is the
 * view's root, a child's path is its parent's plus `.` and its index (`"0.1.0"`). Widget nodes are kept verbatim;
 * containers keep their props, `sizes` staying one weight per child.
 */

import { isObject } from "../sdk/json.ts";
import { strings } from "../strings/index.ts";
import { CONTAINER_KINDS, type ContainerKind, type LayoutNode, type Parsed } from "./layout.ts";
import { DEFAULT_VIEW } from "./views.ts";

/** Where a moved node goes relative to the target: before or after a node, or at the end of a container. */
export type Place = "before" | "after" | "end";

export type LayoutEdit =
  | { readonly kind: "move"; readonly from: string; readonly place: Place; readonly to: string }
  | { readonly kind: "hide"; readonly path: string }
  /** Moves `delta` (a share of the container's total weight) from the cell after the splitter to the one before. */
  | { readonly kind: "resize"; readonly path: string; readonly index: number; readonly delta: number }
  | { readonly kind: "add"; readonly reference: string };

/** One keyboard step of a splitter: 5 % of the container. */
export const RESIZE_STEP = 0.05;

/** The smallest share a resized cell keeps. */
const MIN_SHARE = 0.05;

const text = strings.shell.layout;

interface WidgetItem {
  readonly kind: "widget";
  readonly node: unknown;
}

interface ContainerItem {
  readonly kind: "container";
  readonly head: ContainerKind;
  props: Readonly<Record<string, unknown>>;
  readonly children: Item[];
}

/** A mutable copy of a layout: each node one object, so nodes keep their identity while paths shift. */
type Item = WidgetItem | ContainerItem;

function toItem(node: unknown): Item {
  if (Array.isArray(node)) {
    const [head, props, ...children] = node as unknown[];
    const kind = CONTAINER_KINDS.find((candidate) => candidate === head);
    if (kind !== undefined && isObject(props)) {
      return { kind: "container", head: kind, props, children: children.map(toItem) };
    }
  }
  return { kind: "widget", node };
}

function fromItem(item: Item): LayoutNode {
  if (item.kind === "widget") {
    // A widget node is kept as it was read from a valid layout.
    return item.node as LayoutNode;
  }
  return [item.head, item.props, ...item.children.map(fromItem)];
}

interface Located {
  readonly item: Item;
  /** Containers from the root down to the node's parent; empty for the root. */
  readonly ancestors: readonly ContainerItem[];
}

function locate(root: Item, path: string): Located | undefined {
  const [first, ...rest] = path.split(".");
  if (first !== "0") {
    return undefined;
  }
  let item = root;
  const ancestors: ContainerItem[] = [];
  for (const segment of rest) {
    if (item.kind !== "container" || !/^\d+$/.test(segment)) {
      return undefined;
    }
    const child = item.children[Number(segment)];
    if (child === undefined) {
      return undefined;
    }
    ancestors.push(item);
    item = child;
  }
  return { item, ancestors };
}

function sizesOf(container: ContainerItem): number[] | undefined {
  const { sizes } = container.props;
  return Array.isArray(sizes) ? sizes.filter((size): size is number => typeof size === "number") : undefined;
}

/** Removes `child` from `container` with its size; returns that size (when the container has sizes). */
function detach(container: ContainerItem, child: Item): number | undefined {
  const index = container.children.indexOf(child);
  container.children.splice(index, 1);
  const sizes = sizesOf(container);
  if (sizes === undefined) {
    return undefined;
  }
  const [size] = sizes.splice(index, 1);
  container.props = { ...container.props, sizes };
  return size;
}

/** Inserts `child` at `index`; with sizes, it weighs `size`, else the mean of its new siblings. */
function insert(container: ContainerItem, index: number, child: Item, size: number | undefined): void {
  container.children.splice(index, 0, child);
  const sizes = sizesOf(container);
  if (sizes === undefined) {
    return;
  }
  const mean = sizes.length === 0 ? 1 : round(sizes.reduce((sum, value) => sum + value, 0) / sizes.length);
  sizes.splice(index, 0, size ?? mean);
  container.props = { ...container.props, sizes };
}

/** Removes the containers of `ancestors` (deepest first) that were left without children. */
function prune(ancestors: readonly ContainerItem[]): void {
  for (let depth = ancestors.length - 1; depth >= 1; depth -= 1) {
    const container = ancestors[depth];
    const parent = ancestors[depth - 1];
    if (container === undefined || parent === undefined || container.children.length > 0) {
      return;
    }
    detach(parent, container);
  }
}

function round(value: number): number {
  return Math.round(value * 10_000) / 10_000;
}

function fail(error: string): Parsed<LayoutNode> {
  return { ok: false, error };
}

/** Moves the node at `from` before or after the node at `to`, or to the end of the container at `to`. */
export function moveNode(layout: LayoutNode, from: string, place: Place, to: string): Parsed<LayoutNode> {
  const root = toItem(layout);
  const source = locate(root, from);
  const target = locate(root, to);
  if (source === undefined) {
    return fail(text.noNode(from));
  }
  if (target === undefined) {
    return fail(text.noNode(to));
  }
  const sourceParent = source.ancestors.at(-1);
  if (sourceParent === undefined) {
    return fail(text.root);
  }
  if (to === from || to.startsWith(`${from}.`)) {
    return fail(text.intoItself);
  }
  const container = place === "end" ? target.item : target.ancestors.at(-1);
  if (container === undefined) {
    return fail(text.root);
  }
  if (container.kind !== "container") {
    return fail(text.notContainer(to));
  }
  const size = detach(sourceParent, source.item);
  const index =
    place === "end" ? container.children.length : container.children.indexOf(target.item) + (place === "after" ? 1 : 0);
  insert(container, index, source.item, container === sourceParent ? size : undefined);
  prune(source.ancestors);
  return { ok: true, value: fromItem(root) };
}

/** Removes the node at `path`; a container left without children goes too; the view's last widget stays. */
export function hideNode(layout: LayoutNode, path: string): Parsed<LayoutNode> {
  const root = toItem(layout);
  const found = locate(root, path);
  if (found === undefined) {
    return fail(text.noNode(path));
  }
  const parent = found.ancestors.at(-1);
  if (parent === undefined) {
    return fail(text.lastWidget);
  }
  detach(parent, found.item);
  prune(found.ancestors);
  if (root.kind === "container" && root.children.length === 0) {
    return fail(text.lastWidget);
  }
  return { ok: true, value: fromItem(root) };
}

/**
 * Resizes the two cells of the `row` or `col` at `path` around the splitter before cell `index`: `delta` (a share of
 * the container's total weight, positive to grow the cell before the splitter) moves from one cell to the other;
 * each keeps at least 5 % of the container.
 */
export function resizeCells(layout: LayoutNode, path: string, index: number, delta: number): Parsed<LayoutNode> {
  const root = toItem(layout);
  const found = locate(root, path);
  if (found === undefined) {
    return fail(text.noNode(path));
  }
  const container = found.item;
  if (container.kind !== "container" || (container.head !== "row" && container.head !== "col")) {
    return fail(text.notContainer(path));
  }
  if (!Number.isInteger(index) || index < 1 || index >= container.children.length) {
    return fail(text.notSplit(path, index));
  }
  const sizes = resizeSizes(sizesOf(container) ?? container.children.map(() => 1), index, delta);
  container.props = { ...container.props, sizes };
  return { ok: true, value: fromItem(root) };
}

/**
 * `sizes` after moving `delta` (a share of their total) from cell `index` to cell `index - 1`; both cells keep at
 * least 5 % of the total. The renderer previews a pointer drag with it.
 */
export function resizeSizes(sizes: readonly number[], index: number, delta: number): number[] {
  const next = [...sizes];
  const before = next[index - 1] ?? 1;
  const after = next[index] ?? 1;
  const total = next.reduce((sum, value) => sum + value, 0);
  const pair = before + after;
  const least = Math.min(total * MIN_SHARE, pair / 2);
  const grown = round(Math.min(Math.max(before + delta * total, least), pair - least));
  next[index - 1] = grown;
  next[index] = round(pair - grown);
  return next;
}

/** Adds `reference` at the end of the root container; a root widget becomes a `row` of both (UI-104). */
export function addWidget(layout: LayoutNode, reference: string): LayoutNode {
  const root = toItem(layout);
  if (root.kind === "widget") {
    return ["row", {}, layout, reference];
  }
  insert(root, root.children.length, { kind: "widget", node: reference }, undefined);
  return fromItem(root);
}

export function applyEdit(layout: LayoutNode, edit: LayoutEdit): Parsed<LayoutNode> {
  switch (edit.kind) {
    case "move":
      return moveNode(layout, edit.from, edit.place, edit.to);
    case "hide":
      return hideNode(layout, edit.path);
    case "resize":
      return resizeCells(layout, edit.path, edit.index, edit.delta);
    case "add":
      return { ok: true, value: addWidget(layout, edit.reference) };
  }
}

/** What the views topic says about the workspace's manifest (a `ViewsState`). */
export interface ManifestSource {
  /** The valid manifest; `null` or absent without a file (or when it is invalid). */
  readonly manifest?: unknown;
  /** Why the file is refused; `null` or absent when valid or missing. */
  readonly error?: string | null;
}

/**
 * The manifest `views.set` writes for `edit` on the view `viewId`: the current manifest with that view's layout
 * changed, or, for a workspace without `views.json`, a new manifest holding the default view with the change. An
 * invalid file is never overwritten: the edit is refused until it is fixed.
 */
export function manifestToWrite(source: ManifestSource, viewId: string, edit: LayoutEdit): Parsed<unknown> {
  if (source.error !== undefined && source.error !== null) {
    return { ok: false, error: text.invalidManifest };
  }
  const { manifest } = source;
  if (manifest === undefined || manifest === null) {
    const layout = applyEdit(DEFAULT_VIEW.layout, edit);
    if (!layout.ok) {
      return layout;
    }
    return {
      ok: true,
      value: {
        $schema: "omfp://schemas/views.json",
        default: DEFAULT_VIEW.id,
        views: { [DEFAULT_VIEW.id]: { title: DEFAULT_VIEW.title, layout: layout.value } },
      },
    };
  }
  const views = isObject(manifest) ? manifest.views : undefined;
  const view = isObject(views) ? views[viewId] : undefined;
  if (!isObject(manifest) || !isObject(views) || !isObject(view)) {
    return { ok: false, error: text.noView(viewId) };
  }
  // The manifest is valid (the daemon pushes only valid ones), so the layout is a node.
  const layout = applyEdit(view.layout as LayoutNode, edit);
  if (!layout.ok) {
    return layout;
  }
  return { ok: true, value: { ...manifest, views: { ...views, [viewId]: { ...view, layout: layout.value } } } };
}
