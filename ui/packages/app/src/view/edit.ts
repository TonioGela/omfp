/**
 * What the window writes into `views.json` itself: a widget added with `:widget add` (UI-300), written with
 * `views.set` before the window renders what the daemon pushes back. Splitter sizes stay this window's own (UI-206):
 * {@link resizeSizes} computes them. Agents change views through the daemon (`views.mount`, `views.set`, AGT-061).
 */

import { isObject } from "../sdk/json.ts";
import { strings } from "../strings/index.ts";
import { CONTAINER_KINDS, type LayoutNode, type Parsed } from "./layout.ts";
import { DEFAULT_MANIFEST, DEFAULT_VIEW } from "./views.ts";

/** One keyboard step of a splitter: 5 % of the container. */
export const RESIZE_STEP = 0.05;

/** The smallest share a resized cell keeps. */
const MIN_SHARE = 0.05;

/** The `id` of the `tabs` container widgets are added to (UI-300). */
export const SIDE_TABS = "side";

const text = strings.shell.layout;

function round(value: number): number {
  return Math.round(value * 10_000) / 10_000;
}

/**
 * `sizes` after moving `delta` (a share of their total) from cell `index` to cell `index - 1`; both cells keep at
 * least 5 % of the total. A splitter's drag, keys and preview use it.
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

/** A container node's kind, props and children; `undefined` for a widget node. */
function container(
  node: LayoutNode,
): { kind: string; props: Record<string, unknown>; children: LayoutNode[] } | undefined {
  if (!Array.isArray(node)) {
    return undefined;
  }
  const [kind, props, ...children] = node as readonly unknown[];
  return typeof kind === "string" && CONTAINER_KINDS.some((candidate) => candidate === kind) && isObject(props)
    ? { kind, props, children: children as LayoutNode[] }
    : undefined;
}

/** `props` with one more weight in `sizes` (the mean of the others) when it has sizes. */
function grownSizes(props: Record<string, unknown>): Record<string, unknown> {
  const { sizes } = props;
  if (!Array.isArray(sizes)) {
    return props;
  }
  const weights = sizes.filter((size): size is number => typeof size === "number");
  const mean = weights.length === 0 ? 1 : round(weights.reduce((sum, value) => sum + value, 0) / weights.length);
  return { ...props, sizes: [...weights, mean] };
}

/** `node` with `reference` as the last tab of its first `tabs` container with `"id": "side"`; `undefined`: none. */
function addToSide(node: LayoutNode, reference: string): LayoutNode | undefined {
  const found = container(node);
  if (found === undefined) {
    return undefined;
  }
  if (found.kind === "tabs" && found.props.id === SIDE_TABS) {
    return [found.kind, grownSizes(found.props), ...found.children, reference] as LayoutNode;
  }
  for (const [index, child] of found.children.entries()) {
    const added = addToSide(child, reference);
    if (added !== undefined) {
      const children = found.children.with(index, added);
      return [found.kind, found.props, ...children] as LayoutNode;
    }
  }
  return undefined;
}

/**
 * Adds `reference` as the last tab of the `tabs` with `"id": "side"`, else at the end of the root container (a root
 * widget becomes a `row` of both), as `omfp_view_add` places it (UI-300, DMN-064).
 */
export function addWidget(layout: LayoutNode, reference: string): LayoutNode {
  const side = addToSide(layout, reference);
  if (side !== undefined) {
    return side;
  }
  const root = container(layout);
  if (root === undefined) {
    return ["row", {}, layout, reference];
  }
  return [root.kind, grownSizes(root.props), ...root.children, reference] as LayoutNode;
}

/** What the views topic says about the workspace's manifest (a `ViewsState`). */
export interface ManifestSource {
  /** The valid manifest; `null` or absent without a file (or when it is invalid). */
  readonly manifest?: unknown;
  /** Why the file is refused; `null` or absent when valid or missing. */
  readonly error?: string | null;
}

/**
 * The manifest `views.set` writes to add `reference` to the view `viewId` (UI-300): the current manifest with that
 * view's layout changed, or, for a workspace without `views.json`, a new manifest holding the default view with it. An
 * invalid file is never overwritten: the change is refused until it is fixed.
 */
export function manifestWithWidget(source: ManifestSource, viewId: string, reference: string): Parsed<unknown> {
  if (source.error !== undefined && source.error !== null) {
    return { ok: false, error: text.invalidManifest };
  }
  const { manifest } = source;
  if (manifest === undefined || manifest === null) {
    const layout = addWidget(DEFAULT_VIEW.layout, reference);
    return {
      ok: true,
      value: { ...DEFAULT_MANIFEST, views: { [DEFAULT_VIEW.id]: { title: DEFAULT_VIEW.title, layout } } },
    };
  }
  const views = isObject(manifest) ? manifest.views : undefined;
  const view = isObject(views) ? views[viewId] : undefined;
  if (!isObject(manifest) || !isObject(views) || !isObject(view) || !Object.hasOwn(view, "layout")) {
    return { ok: false, error: text.noView(viewId) };
  }
  // The manifest is valid (the daemon pushes only valid ones), so the layout is a node.
  const layout = addWidget(view.layout as LayoutNode, reference);
  return { ok: true, value: { ...manifest, views: { ...views, [viewId]: { ...view, layout } } } };
}
