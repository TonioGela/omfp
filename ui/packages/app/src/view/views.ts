/**
 * The views a window can show (AGT-010): the built-in default view, or those of a workspace's `.omfp/views.json`
 * manifest, checked in id order with the same rules and messages as the daemon (`fixtures/views/cases.json`).
 */

import { isObject } from "../sdk/json.ts";
import { strings } from "../strings/index.ts";
import { type LayoutNode, type LayoutTree, type Parsed, parseLayout, type View } from "./layout.ts";

/** Header over the conversation, the workspace and changes widgets as tabs on the right; the frame renders the threads
 * column itself (UI-100). */
export const DEFAULT_VIEW: View = {
  id: "default",
  title: strings.shell.frame.defaultView,
  layout: [
    "row",
    { sizes: [3, 1] },
    ["col", {}, "omfp:header", "omfp:chat"],
    ["tabs", {}, "omfp:workspace", "omfp:changes"],
  ],
};

/** A validated view: its layout as written (what layout edits change) and as a tree (what the renderer walks). */
export interface ResolvedView {
  readonly id: string;
  readonly title: string;
  readonly layout: LayoutNode;
  readonly tree: LayoutTree;
}

export interface Views {
  readonly views: readonly ResolvedView[];
  readonly defaultId: string;
  /** Why the manifest was refused; the default view is shown instead. */
  readonly error: string | undefined;
}

function resolveDefault(): ResolvedView {
  const parsed = parseLayout(DEFAULT_VIEW.layout);
  if (!parsed.ok) {
    throw new Error(`the default view is invalid: ${parsed.error}`);
  }
  return { id: DEFAULT_VIEW.id, title: DEFAULT_VIEW.title, layout: DEFAULT_VIEW.layout, tree: parsed.value };
}

const DEFAULT_VIEWS: Views = { views: [resolveDefault()], defaultId: DEFAULT_VIEW.id, error: undefined };

/**
 * The views of `manifest` (`{ default?, views: { <id>: { title, layout } } }`) in id order, or the default view when
 * there is none or it is invalid (with the error, which the frame shows in a banner).
 */
export function resolveViews(manifest: unknown): Views {
  if (manifest === undefined || manifest === null) {
    return DEFAULT_VIEWS;
  }
  const parsed = parseManifest(manifest);
  return parsed.ok ? parsed.value : { ...DEFAULT_VIEWS, error: parsed.error };
}

/** Checks a manifest (AGT-010); the error names the JSON path and the reason, as the daemon's does. */
export function parseManifest(manifest: unknown): Parsed<Views> {
  if (!isObject(manifest) || !isObject(manifest.views)) {
    return { ok: false, error: "views: expected an object of views" };
  }
  const views: ResolvedView[] = [];
  for (const [id, view] of Object.entries(manifest.views).sort(([a], [b]) => compareIds(a, b))) {
    if (!isObject(view)) {
      return { ok: false, error: `views.${id}: expected { title, layout }` };
    }
    const tree = parseLayout(view.layout, `views.${id}.layout`);
    if (!tree.ok) {
      return tree;
    }
    views.push({
      id,
      title: typeof view.title === "string" ? view.title : id,
      // Checked by `parseLayout` just above.
      layout: view.layout as LayoutNode,
      tree: tree.value,
    });
  }
  const [first] = views;
  if (first === undefined) {
    return { ok: false, error: "views: no view" };
  }
  const wanted = manifest.default;
  if (wanted !== undefined && !views.some((view) => view.id === wanted)) {
    return { ok: false, error: `default: no view ${String(wanted)}` };
  }
  return { ok: true, value: { views, defaultId: typeof wanted === "string" ? wanted : first.id, error: undefined } };
}

/** View ids in code unit order (the order the daemon lists them in). */
function compareIds(a: string, b: string): number {
  if (a === b) {
    return 0;
  }
  return a < b ? -1 : 1;
}
