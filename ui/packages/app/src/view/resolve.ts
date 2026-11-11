/**
 * What a widget reference of a view renders (AGT-010, AGT-022, AGT-023): a built-in component, a workspace or user
 * widget module, or nothing (an unknown reference shows an error in its place).
 */

import type { ViewsState, WidgetInfo } from "@omfp/protocol";
import type { WidgetComponent } from "../sdk/index.ts";

/** The built-in `omfp:*` widgets by reference. */
export type WidgetRegistry = ReadonlyMap<string, WidgetComponent>;

export type ResolvedWidget =
  | { readonly kind: "builtin"; readonly component: WidgetComponent }
  /** A widget folder the daemon serves (its `entry` is `null` when the folder has none). */
  | { readonly kind: "module"; readonly widget: WidgetInfo }
  /** A workspace or user widget while the workspace's views state has not arrived yet. */
  | { readonly kind: "pending" }
  | { readonly kind: "missing" };

const BUILTIN = "omfp:";
const WORKSPACE = "./widgets/";
const USER = "user:";

/** The widget folder `name` of `source`, from the views state's list. */
function folder(views: ViewsState, source: WidgetInfo["source"], name: string): WidgetInfo | undefined {
  return views.widgets.find((widget) => widget.source === source && widget.name === name);
}

/**
 * Resolves `reference` in the workspace `workspaceId` with its views state (`undefined` until it arrives):
 * `omfp:<name>` is the workspace widget `<name>` when that folder exists (AGT-022), else the built-in;
 * `./widgets/<name>` the workspace's folder; `user:<name>` the user library's (AGT-023).
 */
export function resolveWidget(
  reference: string,
  workspaceId: string | undefined,
  views: ViewsState | undefined,
  registry: WidgetRegistry,
): ResolvedWidget {
  if (reference.startsWith(BUILTIN)) {
    const override = views === undefined ? undefined : folder(views, "workspace", reference.slice(BUILTIN.length));
    if (override !== undefined) {
      return { kind: "module", widget: override };
    }
    const component = registry.get(reference);
    return component === undefined ? { kind: "missing" } : { kind: "builtin", component };
  }
  const [source, name] = reference.startsWith(WORKSPACE)
    ? (["workspace", reference.slice(WORKSPACE.length)] as const)
    : reference.startsWith(USER)
      ? (["user", reference.slice(USER.length)] as const)
      : [undefined, ""];
  if (source === undefined || name === "" || workspaceId === undefined) {
    return { kind: "missing" };
  }
  if (views === undefined) {
    return { kind: "pending" };
  }
  const widget = folder(views, source, name);
  return widget === undefined ? { kind: "missing" } : { kind: "module", widget };
}

/** The URL of a widget's entry (DMN-084): `<widgetBase>/<workspace id | user>/<name>/<version>/<entry>`. */
export function widgetUrl(widgetBase: string, workspaceId: string, widget: WidgetInfo, entry: string): string {
  return `${widgetModulePath(widgetBase, workspaceId, widget)}${encodeURIComponent(widget.version)}/${encodeURIComponent(entry)}`;
}

/** What every URL of a widget's modules starts with, whatever the version: `<widgetBase>/<workspace id | user>/<name>/`
 * (stacks naming it are the widget's, AGT-026). */
export function widgetModulePath(widgetBase: string, workspaceId: string, widget: WidgetInfo): string {
  const source = widget.source === "user" ? "user" : workspaceId;
  return `${widgetBase}/${encodeURIComponent(source)}/${encodeURIComponent(widget.name)}/`;
}
