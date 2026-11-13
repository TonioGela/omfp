/**
 * Renders a validated view (AGT-010): containers become flex rows and columns, tabs or stacked layers; each widget
 * renders in its own slot, so a failing widget shows its error in place and the rest of the window keeps working.
 * Splitters between the cells of each `row` and `col` resize them for this window (UI-206); `views.json` (or the TSX
 * view) keeps the defaults. On a phone (UI-204) the view shows one area at a time and its rows as tabs, without
 * splitters.
 */

import { Tab, TabList, TabPanel, Tabs } from "@omfp/kit";
import type { ViewsState } from "@omfp/protocol";
import { Fragment, type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from "react";
import {
  fileName,
  fileTabId,
  openedTabIds,
  REVEAL_WIDGET,
  useFileTabs,
  useWindowState,
  widgetTabId,
} from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { resizeSizes } from "./edit.ts";
import {
  type Area,
  chatCell,
  containsWidget,
  FILE_WIDGET,
  fileProps,
  hasMainSlot,
  type LayoutTree,
  MAIN_SLOT,
} from "./layout.ts";
import { OpenedTabLabel, openedTabBarHandlers } from "./opened-tab.tsx";
import type { WidgetRegistry } from "./resolve.ts";
import { effectiveSizes, PANE_SIZES, sizesKey, withSizes } from "./sizes.ts";
import { WidgetSlot } from "./slot.tsx";
import { Resizer } from "./splitter.tsx";
import type { ResolvedView } from "./views.ts";

const text = strings.shell.frame;
const layoutText = strings.shell.layout;

export interface ViewRendererProps {
  readonly view: ResolvedView;
  readonly registry: WidgetRegistry;
  /** The workspace whose view this is; `undefined` without a current workspace. */
  readonly workspaceId?: string | undefined;
  /** That workspace's views state (widget folders, stored state); `undefined` until it arrives. */
  readonly views?: ViewsState | undefined;
  /** On a phone (UI-204): the area shown; absent on wider frames. */
  readonly area?: Area | undefined;
}

/** What every node of one render shares. */
interface Scope {
  readonly registry: WidgetRegistry;
  readonly workspaceId: string | undefined;
  readonly views: ViewsState | undefined;
  readonly viewId: string;
  /** On a phone: the area shown. */
  readonly area: Area | undefined;
}

export function ViewRenderer({ view, registry, workspaceId, views, area }: ViewRendererProps): ReactNode {
  const scope: Scope = { registry, workspaceId, views, viewId: view.id, area };
  return (
    <div className="omfp-view" data-testid="view" data-view={view.id} data-area={area}>
      <Node tree={view.tree} path="0" scope={scope} />
    </div>
  );
}

export interface PanelLayoutProps {
  /** The panel's id (AGT-060): its nodes' paths start at `panel:<id>`. */
  readonly id: string;
  readonly tree: LayoutTree;
  readonly registry: WidgetRegistry;
  readonly workspaceId: string | undefined;
  readonly views: ViewsState | undefined;
}

/** A floating panel's node (UI-230), rendered like a view's nodes. */
export function PanelLayout({ id, tree, registry, workspaceId, views }: PanelLayoutProps): ReactNode {
  const path = `panel:${id}`;
  const scope: Scope = { registry, workspaceId, views, viewId: path, area: undefined };
  return <Node tree={tree} path={path} scope={scope} />;
}

function Node({ tree, path, scope }: { tree: LayoutTree; path: string; scope: Scope }): ReactNode {
  if (tree.kind === "widget") {
    return (
      <div className="omfp-view__item">
        <WidgetSlot
          reference={tree.name}
          props={tree.props}
          path={path}
          workspaceId={scope.workspaceId}
          views={scope.views}
          registry={scope.registry}
        />
      </div>
    );
  }
  if (tree.kind === "tabs") {
    if (tree.props.id === MAIN_SLOT) {
      return <MainTabs tree={tree} path={path} scope={scope} />;
    }
    return <TabsNode tree={tree} path={path} scope={scope} />;
  }
  return <Cells tree={tree} path={path} scope={scope} />;
}

/**
 * Selects the tab whose subtree holds the widget of the window's latest reveal request (UI-274), when it arrives.
 * `tabs` are the container's own children with their tab ids.
 */
function useRevealTab(tabs: readonly { child: LayoutTree; id: string }[], select: (id: string) => void): void {
  const [request] = useWindowState(REVEAL_WIDGET);
  const latest = useRef(select);
  latest.current = select;
  const target =
    request === undefined ? undefined : tabs.find(({ child }) => containsWidget(child, [request.reference]))?.id;
  useEffect(() => {
    if (request !== undefined && target !== undefined) {
      latest.current(target);
    }
  }, [request, target]);
}

/** A `tabs` container other than `main`: one tab per child, the selected one window-local. */
function TabsNode({
  tree,
  path,
  scope,
}: {
  tree: Exclude<LayoutTree, { kind: "widget" }>;
  path: string;
  scope: Scope;
}): ReactNode {
  const children = tree.children.map((child, index) => ({ child, id: `${path}.${index}` }));
  const [picked, setPicked] = useState<string | undefined>(undefined);
  useRevealTab(children, setPicked);
  const selected = children.find(({ id }) => id === picked)?.id ?? children[0]?.id;
  const [only] = children;
  if (only !== undefined && children.length === 1) {
    // One tab needs no tab bar (AGT-010): the default view's `main` slot holds the chat alone until files open.
    return (
      <div className="omfp-view__tabs" data-layout="tabs" data-path={path}>
        <div className="omfp-view__panel">
          <Node tree={only.child} path={only.id} scope={scope} />
        </div>
      </div>
    );
  }
  return (
    <Tabs
      className="omfp-view__tabs"
      data-layout="tabs"
      data-path={path}
      {...(selected === undefined ? {} : { selectedKey: selected })}
      onSelectionChange={(key) => setPicked(String(key))}
    >
      <TabList aria-label={text.views}>
        {children.map(({ child, id }) => (
          <Tab key={id} id={id} data-testid="view-tab">
            {nodeTitle(child)}
          </Tab>
        ))}
      </TabList>
      {children.map(({ child, id }) => (
        <TabPanel key={id} id={id} className="omfp-view__panel">
          <Node tree={child} path={id} scope={scope} />
        </TabPanel>
      ))}
    </Tabs>
  );
}

/**
 * The `tabs` container with `"id": "main"` (UI-115): its own children, then this window's open files of the workspace
 * as tabs (`omfp:file`), then the widgets reveals opened here (UI-373), the selected one window-local too. One tab
 * alone needs no tab bar. A middle click closes the opened tab under the pointer (UI-365), as browsers and editors do.
 */
function MainTabs({
  tree,
  path,
  scope,
}: {
  tree: Exclude<LayoutTree, { kind: "widget" }>;
  path: string;
  scope: Scope;
}): ReactNode {
  const files = useFileTabs(scope.workspaceId);
  const own = tree.children.map((child, index) => ({ child, id: `${path}.${index}` }));
  useRevealTab(own, files.select);
  const ids = [...own.map(({ id }) => id), ...openedTabIds(files)];
  const selected = files.active !== undefined && ids.includes(files.active) ? files.active : ids[0];
  const [only] = own;
  if (only !== undefined && ids.length === 1) {
    return (
      <div className="omfp-view__tabs" data-layout="tabs" data-path={path} data-slot={MAIN_SLOT}>
        <div className="omfp-view__panel">
          <Node tree={only.child} path={only.id} scope={scope} />
        </div>
      </div>
    );
  }
  // A widget tab has no props: its instance (useStored, AGT-018) is its reference, as a view node without `instance`.
  const opened = [
    ...files.tabs.map((tab) => ({
      id: fileTabId(tab.path),
      kind: "file" as const,
      title: fileName(tab.path),
      tooltip: tab.path,
      reference: FILE_WIDGET,
      props: fileProps(tab),
      file: tab.path,
    })),
    ...files.widgets.map((reference) => ({
      id: widgetTabId(reference),
      kind: "widget" as const,
      title: widgetTitle(reference),
      tooltip: reference,
      reference,
      props: {},
      file: undefined,
    })),
  ];
  return (
    <Tabs
      className="omfp-view__tabs"
      data-layout="tabs"
      data-path={path}
      data-slot={MAIN_SLOT}
      {...(selected === undefined ? {} : { selectedKey: selected })}
      onSelectionChange={(key) => files.select(String(key))}
    >
      <div className="omfp-view__tabbar" {...openedTabBarHandlers(files.closeTab)}>
        <TabList aria-label={text.views}>
          {[
            ...own.map(({ child, id }) => (
              <Tab key={id} id={id} data-testid="view-tab">
                {nodeTitle(child)}
              </Tab>
            )),
            ...opened.map((tab) => (
              <Tab key={tab.id} id={tab.id} data-testid={`${tab.kind}-tab`} data-tab={tab.id} data-file={tab.file}>
                <OpenedTabLabel
                  id={tab.id}
                  title={tab.title}
                  tooltip={tab.tooltip}
                  kind={tab.kind}
                  onClose={files.closeTab}
                />
              </Tab>
            )),
          ]}
        </TabList>
      </div>
      {own.map(({ child, id }) => (
        <TabPanel key={id} id={id} className="omfp-view__panel">
          <Node tree={child} path={id} scope={scope} />
        </TabPanel>
      ))}
      {opened.map((tab) => (
        <TabPanel key={tab.id} id={tab.id} className="omfp-view__panel">
          <div className="omfp-view__item">
            <WidgetSlot
              reference={tab.reference}
              props={tab.props}
              path={tab.id}
              workspaceId={scope.workspaceId}
              views={scope.views}
              registry={scope.registry}
            />
          </div>
        </TabPanel>
      ))}
    </Tabs>
  );
}

/** Whether `tree` holds the conversation (the `main` slot or `omfp:chat`). */
function holdsChat(tree: LayoutTree): boolean {
  return hasMainSlot(tree) || containsWidget(tree, ["omfp:chat"]);
}

/** Widgets as tall as their content in a `col` (styles.css), which a splitter cannot resize. */
const FIXED_WIDGETS = ["omfp:header"];

/**
 * A `row`, `col` or `stack`: its children side by side, stacked or layered, with a splitter between the cells of a
 * `row` or `col`. A splitter resizes the cells for this window (kept by workspace, view and path; a double click goes
 * back to the view's sizes). On a phone the cells of a row are tabs (all mounted, one shown) and the view's root row
 * shows the chat area or the side area (UI-204).
 */
function Cells({
  tree,
  path,
  scope,
}: {
  tree: Exclude<LayoutTree, { kind: "widget" }>;
  path: string;
  scope: Scope;
}): ReactNode {
  const { area } = scope;
  const key = sizesKey(scope.workspaceId, scope.viewId, path);
  const [own, setOwn] = useWindowState(PANE_SIZES);
  const count = tree.children.length;
  const sizes = effectiveSizes(tree.props.sizes, own[key], count);
  // A pointer drag's sizes, shown until the window's new sizes arrive.
  const [preview, setPreview] = useState<{ base: readonly number[] | undefined; sizes: readonly number[] }>();
  const shown = preview !== undefined && preview.base === sizes ? preview.sizes : sizes;
  const current = sizes ?? tree.children.map(() => 1);
  const phone = area !== undefined;
  const resizable = !phone && (tree.kind === "row" || tree.kind === "col");
  const fixed = (index: number) => {
    const child = tree.children[index];
    return tree.kind === "col" && child?.kind === "widget" && FIXED_WIDGETS.includes(child.name);
  };
  // Every `row` is tabs on a phone, and a `col` without the conversation (a header over the chat stays stacked).
  const boxes = tree.kind === "col" && !holdsChat(tree);
  const tabbed = phone && (tree.kind === "row" || boxes);
  const chat = tabbed && path === "0" ? chatCell(tree) : undefined;
  const indices = tree.children.map((_, index) => index);
  const choices = chat === undefined ? indices : area === "chat" ? [chat] : indices.filter((index) => index !== chat);
  const [picked, setPicked] = useState<number>();
  const selected = picked !== undefined && choices.includes(picked) ? picked : (choices[0] ?? 0);
  const resize = (index: number, delta: number) => {
    setPreview(undefined);
    setOwn((previous) => withSizes(previous, key, resizeSizes(current, index, delta)));
  };
  return (
    <div
      className={`omfp-view__${tree.kind}`}
      data-layout={tree.kind}
      data-path={path}
      data-tabbed={tabbed || undefined}
      data-boxes={(boxes && !tabbed) || undefined}
    >
      {tabbed && choices.length > 1 && (
        <CellTabs tree={tree} path={path} choices={choices} selected={selected} onSelect={setPicked} />
      )}
      {tree.children.map((child, index) => {
        const id = `${path}.${index}`;
        const before = shown?.[index - 1] ?? 1;
        const after = shown?.[index] ?? 1;
        return (
          <Fragment key={id}>
            {resizable && index > 0 && !fixed(index - 1) && !fixed(index) && (
              <Resizer
                orientation={tree.kind === "row" ? "vertical" : "horizontal"}
                label={layoutText.splitter(tree.kind, path, index)}
                value={(before / (before + after)) * 100}
                data-testid="splitter"
                data-path={path}
                data-index={index}
                onPreview={(delta) =>
                  setPreview(
                    delta === undefined ? undefined : { base: sizes, sizes: resizeSizes(current, index, delta) },
                  )
                }
                onResize={(delta) => resize(index, delta)}
                onReset={() => {
                  setPreview(undefined);
                  setOwn((previous) => withSizes(previous, key, undefined));
                }}
              />
            )}
            <div
              className="omfp-view__cell"
              id={tabbed ? cellId(id) : undefined}
              role={tabbed ? "tabpanel" : undefined}
              hidden={tabbed && index !== selected}
              style={shown === undefined || tabbed ? undefined : { flexGrow: shown[index] }}
            >
              <Node tree={child} path={id} scope={scope} />
            </div>
          </Fragment>
        );
      })}
    </div>
  );
}

function cellId(path: string): string {
  return `omfp-cell-${path.replaceAll(".", "-")}`;
}

/**
 * The tab bar of a container shown as tabs on a phone (UI-204): one tab per cell it may show, labelled like the
 * `tabs` container's (UI-154); the arrow keys move between them.
 */
function CellTabs({
  tree,
  path,
  choices,
  selected,
  onSelect,
}: {
  tree: Exclude<LayoutTree, { kind: "widget" }>;
  path: string;
  choices: readonly number[];
  selected: number;
  onSelect: (index: number) => void;
}): ReactNode {
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = { ArrowLeft: -1, ArrowRight: 1 }[event.key];
    if (step === undefined) {
      return;
    }
    event.preventDefault();
    const next = choices[(choices.indexOf(selected) + step + choices.length) % choices.length] ?? selected;
    onSelect(next);
    event.currentTarget.querySelector<HTMLElement>(`[data-index="${next}"]`)?.focus();
  };
  return (
    <div
      className="omfp-tabs__list omfp-view__celltabs"
      role="tablist"
      aria-label={strings.shell.responsive.tabs(tree.kind, path)}
      onKeyDown={onKeyDown}
    >
      {choices.map((index) => {
        const child = tree.children[index];
        const id = `${path}.${index}`;
        return (
          <button
            key={id}
            type="button"
            role="tab"
            className="omfp-tabs__tab"
            aria-selected={index === selected}
            aria-controls={cellId(id)}
            tabIndex={index === selected ? 0 : -1}
            data-selected={index === selected || undefined}
            data-testid="cell-tab"
            data-index={index}
            onClick={() => onSelect(index)}
          >
            {child === undefined ? id : nodeTitle(child)}
          </button>
        );
      })}
    </div>
  );
}

/** A tab's label: the widget's `title` prop, else {@link widgetTitle}; a container's kind. */
function nodeTitle(tree: LayoutTree): string {
  if (tree.kind !== "widget") {
    return tree.kind;
  }
  return typeof tree.props.title === "string" ? tree.props.title : widgetTitle(tree.name);
}

/** A widget's title without a `title` prop: a built-in's own name, else its reference. */
export function widgetTitle(reference: string): string {
  return Object.hasOwn(text.widgetTitles, reference) ? (text.widgetTitles[reference] ?? reference) : reference;
}
