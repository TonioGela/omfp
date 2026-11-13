/**
 * The layout components of TSX views (UI-268, experimental, branch idea/init): a view file's default export returns
 * one `Row`, `Col`, `Tabs`, `Stack` or `Widget` element nesting the others, which the window turns into the same
 * layout node `views.json` holds (`<Row sizes={[3, 1]}>` is `["row", { "sizes": [3, 1] }, …]`, `<Widget use="omfp:chat"
 * props={{ … }} />` is `["omfp:chat", { … }]`) and renders like any view. The view component is a React component:
 * its hooks work, and a re-render gives the window a new layout.
 */

import {
  Children,
  createContext,
  Fragment,
  isValidElement,
  type ReactElement,
  type ReactNode,
  useContext,
  useEffect,
  useId,
} from "react";
import { strings } from "../strings/index.ts";
import type { ContainerKind, LayoutNode, Parsed } from "../view/layout.ts";
import type { WidgetProps } from "./widget.ts";

const text = strings.init.viewFile;

/** `Row`, `Col` and `Stack`: `sizes` are the cells' default flex weights (the window's own sizes apply on top). */
export interface ContainerElementProps {
  readonly sizes?: readonly number[];
  readonly children?: ReactNode;
}

/** `Tabs`: `id: "main"` makes it the slot files open in (UI-115). */
export interface TabsElementProps extends ContainerElementProps {
  readonly id?: string;
}

/** `Widget`: a widget reference (`omfp:chat`, `./widgets/deploy`, `user:clock`) and its props. */
export interface WidgetElementProps {
  readonly use: string;
  readonly props?: WidgetProps;
}

/** Where the rendered layouts of a view file go: one entry per rendered root, by id. */
export interface LayoutCollector {
  set(id: string, layout: Parsed<LayoutNode> | undefined): void;
}

export const LayoutCollectorContext = createContext<LayoutCollector | undefined>(undefined);

const KINDS = new Map<unknown, ContainerKind | "widget">();

/** The layout node of `element`, one of the layout components; throws on anything else. */
function toNode(element: ReactNode): LayoutNode[] {
  if (element === null || element === undefined || typeof element === "boolean") {
    return [];
  }
  if (!isValidElement(element)) {
    throw new Error(text.text);
  }
  const { type } = element as ReactElement<{ children?: ReactNode }>;
  if (type === Fragment) {
    return Children.toArray((element as ReactElement<{ children?: ReactNode }>).props.children).flatMap(toNode);
  }
  const kind = KINDS.get(type);
  if (kind === undefined) {
    const name =
      typeof type === "string"
        ? `<${type}>`
        : typeof type === "function" && type.name !== ""
          ? type.name
          : "a component";
    throw new Error(text.notLayout(name));
  }
  return [nodeOf(kind, element.props as Record<string, unknown>)];
}

function nodeOf(kind: ContainerKind | "widget", props: Record<string, unknown>): LayoutNode {
  if (kind === "widget") {
    const { use, props: widgetProps } = props;
    if (typeof use !== "string") {
      throw new Error(text.noUse);
    }
    return widgetProps === undefined ? use : [use, widgetProps as WidgetProps];
  }
  const { children, ...rest } = props;
  return [kind, rest, ...Children.toArray(children as ReactNode).flatMap(toNode)] as LayoutNode;
}

/** Reports the layout this rendered root stands for; only a root renders, its children are read as elements. */
function useRoot(kind: ContainerKind | "widget", props: object, name: string): null {
  const collector = useContext(LayoutCollectorContext);
  const id = useId();
  if (collector === undefined) {
    throw new Error(text.outside(name));
  }
  let layout: Parsed<LayoutNode>;
  try {
    layout = { ok: true, value: nodeOf(kind, props as Record<string, unknown>) };
  } catch (error) {
    layout = { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
  const key = JSON.stringify(layout);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` stands for `layout`
  useEffect(() => collector.set(id, layout), [collector, id, key]);
  useEffect(() => () => collector.set(id, undefined), [collector, id]);
  return null;
}

/** A row of cells side by side, a splitter between each two. */
export function Row(props: ContainerElementProps): null {
  return useRoot("row", props, "Row");
}

/** A column of cells, a splitter between each two. */
export function Col(props: ContainerElementProps): null {
  return useRoot("col", props, "Col");
}

/** One tab per child; `id="main"` is where files open. */
export function Tabs(props: TabsElementProps): null {
  return useRoot("tabs", props, "Tabs");
}

/** Its children layered over each other. */
export function Stack(props: ContainerElementProps): null {
  return useRoot("stack", props, "Stack");
}

/** A widget: `use` is its reference, `props` its props. */
export function Widget(props: WidgetElementProps): null {
  return useRoot("widget", props, "Widget");
}

KINDS.set(Row, "row");
KINDS.set(Col, "col");
KINDS.set(Tabs, "tabs");
KINDS.set(Stack, "stack");
KINDS.set(Widget, "widget");
