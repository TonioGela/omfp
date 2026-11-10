/** The views a window can show: the built-in default view, or those of a `views.json`-shaped manifest (M4). */

import { isObject } from "../sdk/json.ts";
import { strings } from "../strings/index.ts";
import { type LayoutTree, type Parsed, parseLayout, type View } from "./layout.ts";

/** Threads on the left, header and conversation in the middle, the workspace on the right. */
export const DEFAULT_VIEW: View = {
  id: "default",
  title: strings.shell.frame.defaultView,
  layout: ["row", { sizes: [1, 3, 1] }, "omfp:threads", ["col", {}, "omfp:header", "omfp:chat"], "omfp:workspace"],
};

/** A validated view. */
export interface ResolvedView {
  readonly id: string;
  readonly title: string;
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
  return { id: DEFAULT_VIEW.id, title: DEFAULT_VIEW.title, tree: parsed.value };
}

const DEFAULT_VIEWS: Views = { views: [resolveDefault()], defaultId: DEFAULT_VIEW.id, error: undefined };

/**
 * The views of `manifest` (`{ default?, views: { <id>: { title, layout } } }`), or the default view when there is none
 * or it is invalid (with the error, which the frame shows in a banner).
 */
export function resolveViews(manifest: unknown): Views {
  if (manifest === undefined) {
    return DEFAULT_VIEWS;
  }
  const parsed = parseManifest(manifest);
  return parsed.ok ? parsed.value : { ...DEFAULT_VIEWS, error: parsed.error };
}

function parseManifest(manifest: unknown): Parsed<Views> {
  if (!isObject(manifest) || !isObject(manifest.views)) {
    return { ok: false, error: "views: expected an object of views" };
  }
  const views: ResolvedView[] = [];
  for (const [id, view] of Object.entries(manifest.views)) {
    if (!isObject(view)) {
      return { ok: false, error: `views.${id}: expected { title, layout }` };
    }
    const tree = parseLayout(view.layout, `views.${id}.layout`);
    if (!tree.ok) {
      return tree;
    }
    views.push({ id, title: typeof view.title === "string" ? view.title : id, tree: tree.value });
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
