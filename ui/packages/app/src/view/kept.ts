/**
 * The views a window keeps mounted (UI-390): the one it shows and the {@link KEPT_VIEWS} of the current workspace it
 * showed most recently before, which the frame renders hidden (React's `<Activity mode="hidden">`). A hidden view
 * keeps its widgets' state and DOM, while its effects are cleaned up (subscriptions, polls, commands and widget
 * reports pause); showing it again shows it as it was at once and runs them again. Views of another workspace and
 * views no longer listed are dropped.
 */

import { isFileView, type ListedView, type ResolvedView } from "./views.ts";

/** How many views besides the shown one stay mounted, hidden: the least recently shown goes first. */
export const KEPT_VIEWS = 2;

export interface KeptView {
  /** The React key of its mount: new for every view kept, except that the first view shown in another workspace
   * takes over the mount of the view shown before (its widgets update in place, as they did before views were
   * kept). */
  readonly key: string;
  /** The id of the switcher's view (a TSX view's, also while the default view stands in for it). */
  readonly id: string;
  /** What it renders: the listed layout, or a TSX view's rendered layout as last shown. */
  readonly view: ResolvedView;
  /** When it was last shown, on the {@link KeptViews.clock}. */
  readonly shown: number;
}

export interface KeptViews {
  readonly workspaceId: string | undefined;
  /** In the order they were first kept, the DOM's order: a kept view never moves (moving a node resets its scroll
   * positions). */
  readonly views: readonly KeptView[];
  /** Counts the shows. */
  readonly clock: number;
}

export const NO_KEPT_VIEWS: KeptViews = { workspaceId: undefined, views: [], clock: 0 };

/**
 * `kept` once the window shows `shown` of `workspaceId` (`undefined`: nothing yet, a TSX view still rendering), whose
 * switcher lists `listed`: the views still listed (written layouts as listed now, TSX views as last rendered), `shown`
 * as the most recent, at most {@link KEPT_VIEWS} others. `kept` itself when nothing changes.
 */
export function keepViews(
  kept: KeptViews,
  workspaceId: string | undefined,
  shown: { readonly id: string; readonly view: ResolvedView } | undefined,
  listed: readonly ListedView[],
): KeptViews {
  const moved = kept.workspaceId !== workspaceId;
  let changed = moved;
  let views: KeptView[] = [];
  for (const entry of moved ? [] : kept.views) {
    const now = listed.find((candidate) => candidate.id === entry.id);
    if (now === undefined) {
      changed = true;
      continue;
    }
    const view = isFileView(now) || sameView(now, entry.view) ? entry.view : now;
    changed ||= view !== entry.view;
    views.push(view === entry.view ? entry : { ...entry, view });
  }
  let { clock } = kept;
  if (shown !== undefined) {
    const index = views.findIndex((entry) => entry.id === shown.id);
    const entry = views[index];
    // The most recently shown view has the clock's time (each show takes the next one).
    if (entry === undefined || !sameView(entry.view, shown.view) || entry.shown !== clock) {
      clock += 1;
      const before = moved ? kept.views.find((candidate) => candidate.shown === kept.clock) : undefined;
      const key = entry?.key ?? before?.key ?? `view-${clock}`;
      const now = { key, id: shown.id, view: shown.view, shown: clock };
      if (entry === undefined) {
        views.push(now);
      } else {
        views[index] = now;
      }
      changed = true;
    }
    while (views.length > KEPT_VIEWS + 1) {
      const oldest = Math.min(...views.map((candidate) => candidate.shown));
      views = views.filter((candidate) => candidate.shown !== oldest);
    }
  }
  return changed ? { workspaceId, views, clock } : kept;
}

/** Whether `a` and `b` render the same: the same layout as written (a TSX view's render or a manifest's), so a new
 * object made of an unchanged layout changes nothing. */
function sameView(a: ResolvedView, b: ResolvedView): boolean {
  return a.id === b.id && a.title === b.title && a.layout === b.layout && a.file === b.file && a.init === b.init;
}
