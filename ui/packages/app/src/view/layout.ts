/**
 * Layout trees (AGT-010): a view's layout is a widget reference (`"omfp:chat"` or `["./widgets/tests", { … }]`) or a
 * container (`["row" | "col" | "tabs" | "stack", { sizes?, … }, ...children]`). Layouts come from data written by
 * agents, so they are validated: an invalid one never breaks the window, the frame shows the default view and the
 * error instead (AGT-011).
 */

import type { WidgetProps } from "../sdk/index.ts";
import { isObject } from "../sdk/json.ts";

export type ContainerKind = "row" | "col" | "tabs" | "stack";

export const CONTAINER_KINDS: readonly ContainerKind[] = ["row", "col", "tabs", "stack"];

/** Container props: `sizes` are flex weights, one per child (equal when absent). */
export interface ContainerProps {
  readonly sizes?: readonly number[];
  readonly [key: string]: unknown;
}

/** A layout node as written in `views.json`. */
export type LayoutNode =
  | string
  | readonly [string, WidgetProps]
  | readonly [ContainerKind, ContainerProps, ...LayoutNode[]];

/** A layout node after validation. */
export type LayoutTree =
  | { readonly kind: "widget"; readonly name: string; readonly props: WidgetProps }
  | {
      readonly kind: ContainerKind;
      readonly props: ContainerProps;
      readonly children: readonly LayoutTree[];
    };

export interface View {
  readonly id: string;
  readonly title: string;
  readonly layout: LayoutNode;
}

export type Parsed<T> = { readonly ok: true; readonly value: T } | { readonly ok: false; readonly error: string };

/** Checks `node` (at `path`, for error messages) and turns it into a {@link LayoutTree}. */
export function parseLayout(node: unknown, path = "layout"): Parsed<LayoutTree> {
  if (typeof node === "string") {
    return node.trim() === ""
      ? { ok: false, error: `${path}: a widget name is empty` }
      : { ok: true, value: { kind: "widget", name: node, props: {} } };
  }
  if (!Array.isArray(node) || node.length === 0 || typeof node[0] !== "string") {
    return { ok: false, error: `${path}: expected a widget name, [widget, props] or [container, props, ...children]` };
  }
  const [head, props, ...children] = node as [string, unknown, ...unknown[]];
  if (!isObject(props)) {
    return { ok: false, error: `${path}: ${head} needs a props object` };
  }
  const kind = CONTAINER_KINDS.find((candidate) => candidate === head);
  if (kind === undefined) {
    return children.length > 0 || head.trim() === ""
      ? { ok: false, error: `${path}: a widget reference is [name, props]` }
      : { ok: true, value: { kind: "widget", name: head, props } };
  }
  return parseContainer(kind, props, children, path);
}

function parseContainer(
  kind: ContainerKind,
  props: Readonly<Record<string, unknown>>,
  children: readonly unknown[],
  path: string,
): Parsed<LayoutTree> {
  if (children.length === 0) {
    return { ok: false, error: `${path}: ${kind} has no children` };
  }
  const { sizes } = props;
  if (
    sizes !== undefined &&
    !(
      Array.isArray(sizes) &&
      sizes.length === children.length &&
      sizes.every((size) => typeof size === "number" && Number.isFinite(size) && size > 0)
    )
  ) {
    return { ok: false, error: `${path}: sizes must be ${children.length} positive numbers` };
  }
  const trees: LayoutTree[] = [];
  for (const [index, child] of children.entries()) {
    const parsed = parseLayout(child, `${path}[${index + 2}]`);
    if (!parsed.ok) {
      return parsed;
    }
    trees.push(parsed.value);
  }
  return { ok: true, value: { kind, props: props as ContainerProps, children: trees } };
}

/** Whether any widget of `tree` is one of `names`. */
export function containsWidget(tree: LayoutTree, names: readonly string[]): boolean {
  return tree.kind === "widget"
    ? names.includes(tree.name)
    : tree.children.some((child) => containsWidget(child, names));
}
