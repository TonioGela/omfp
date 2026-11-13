/**
 * Renders a validated view (AGT-010): containers become flex rows and columns, tabs or stacked layers; each widget
 * renders in its own slot, so a failing widget shows its error in place and the rest of the window keeps working.
 * Splitters between the cells of each `row` and `col` resize them for this window (UI-206); in this window's edit mode
 * (UI-103) they write the view's defaults instead, and every widget gets a toolbar (reference, Move, Hide); each change
 * goes to `onEdit`, and the view then renders what the daemon pushes back. On a phone (UI-204) the view shows one area
 * at a time and its rows as tabs, without splitters.
 */

import {
  Button,
  Icon,
  IconButton,
  Menu,
  MenuItem,
  MenuTrigger,
  Popover,
  Tab,
  TabList,
  TabPanel,
  Tabs,
  Toolbar,
} from "@omfp/kit";
import type { ViewsState } from "@omfp/protocol";
import {
  Fragment,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { fileName, fileTabId, REVEAL_WIDGET, useFileTabs, useWindowState } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { type LayoutEdit, type Place, resizeSizes } from "./edit.ts";
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
import { addedBy, type NodeOrigins } from "./origins.ts";
import type { WidgetRegistry } from "./resolve.ts";
import { effectiveSizes, PANE_SIZES, sizesKey, withSizes } from "./sizes.ts";
import { WidgetSlot } from "./slot.tsx";
import { Resizer } from "./splitter.tsx";
import type { ResolvedView } from "./views.ts";

const text = strings.shell.frame;
const layoutText = strings.shell.layout;
const filesText = strings.shell.files;

export interface ViewRendererProps {
  readonly view: ResolvedView;
  readonly registry: WidgetRegistry;
  /** The workspace whose view this is; `undefined` without a current workspace. */
  readonly workspaceId?: string | undefined;
  /** That workspace's views state (widget folders, stored state); `undefined` until it arrives. */
  readonly views?: ViewsState | undefined;
  /** Edit mode (UI-103): writes a layout edit, resolving to whether it was written; absent outside edit mode. */
  readonly onEdit?: ((edit: LayoutEdit) => Promise<boolean>) | undefined;
  /** On a phone (UI-204): the area shown; absent on wider frames. */
  readonly area?: Area | undefined;
}

/** A place the Move menu offers. */
interface MoveTarget {
  readonly id: string;
  readonly place: Place;
  readonly path: string;
  readonly label: string;
}

/** What every node of one render shares. */
interface Scope {
  readonly registry: WidgetRegistry;
  readonly workspaceId: string | undefined;
  readonly views: ViewsState | undefined;
  readonly onEdit: ((edit: LayoutEdit) => Promise<boolean>) | undefined;
  /** Every place of the view, in tree order (widgets: before and after; containers: at the end). */
  readonly targets: readonly MoveTarget[];
  /** Whether the view holds a single widget (which cannot be hidden). */
  readonly single: boolean;
  readonly viewId: string;
  /** Where the nodes of a `views.json` view init files changed come from (UI-269). */
  readonly origins: NodeOrigins | undefined;
  /** On a phone: the area shown. */
  readonly area: Area | undefined;
  /** Hide was pressed on the toolbar at `index` (document order), or `undefined`: its edit failed; the focus moves on
   * once the view changes. */
  readonly onHide: ((index: number | undefined) => void) | undefined;
}

const TOOLBAR = '[data-testid="widget-toolbar"]';

/**
 * Where the focus goes after Hide (UI-285): to the toolbar now at the hidden widget's place (the next widget's), else
 * the previous one, else the frame's "Edit layout" — once the view without the widget renders, unless the focus moved
 * on meanwhile (outside the toolbars, which the next widget may have taken over in place).
 */
function useFocusAfterHide(tree: LayoutTree): [RefObject<HTMLDivElement | null>, (index: number | undefined) => void] {
  const root = useRef<HTMLDivElement>(null);
  const pending = useRef<number | undefined>(undefined);
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when the view the daemon pushes back renders
  useEffect(() => {
    const index = pending.current;
    pending.current = undefined;
    const active = document.activeElement;
    const lost = active === null || active === document.body || active.closest(TOOLBAR) !== null;
    if (index === undefined || root.current === null || !lost) {
      return;
    }
    const toolbars = [...root.current.querySelectorAll<HTMLElement>(TOOLBAR)];
    const toolbar = toolbars[index] ?? toolbars[index - 1];
    const target =
      toolbar === undefined
        ? document.querySelector<HTMLElement>('[data-testid="layout-edit"]')
        : toolbar.querySelector<HTMLElement>("button:not([disabled])");
    target?.focus();
  }, [tree]);
  return [
    root,
    (index) => {
      pending.current = index;
    },
  ];
}

export function ViewRenderer({ view, registry, workspaceId, views, onEdit, area }: ViewRendererProps): ReactNode {
  // A node an init file added is no Move target (UI-269).
  const targets = useMemo(
    () => moveTargets(view.tree, "0").filter((target) => addedBy(view.origins, target.path) === undefined),
    [view.tree, view.origins],
  );
  const [root, onHide] = useFocusAfterHide(view.tree);
  const scope: Scope = {
    registry,
    workspaceId,
    views,
    onEdit,
    targets,
    single: view.tree.kind === "widget",
    viewId: view.id,
    origins: view.origins,
    area,
    onHide,
  };
  return (
    <div
      ref={root}
      className="omfp-view"
      data-testid="view"
      data-view={view.id}
      data-editing={onEdit !== undefined}
      data-area={area}
    >
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

/** A floating panel's node (UI-230), rendered like a view's nodes, outside edit mode. */
export function PanelLayout({ id, tree, registry, workspaceId, views }: PanelLayoutProps): ReactNode {
  const path = `panel:${id}`;
  const scope: Scope = {
    registry,
    workspaceId,
    views,
    onEdit: undefined,
    targets: [],
    single: true,
    viewId: path,
    origins: undefined,
    area: undefined,
    onHide: undefined,
  };
  return <Node tree={tree} path={path} scope={scope} />;
}

function moveTargets(tree: LayoutTree, path: string): MoveTarget[] {
  if (tree.kind === "widget") {
    return (["before", "after"] as const).map((place) => ({
      id: `${place}:${path}`,
      place,
      path,
      label: place === "before" ? layoutText.before(tree.name, path) : layoutText.after(tree.name, path),
    }));
  }
  return [
    ...tree.children.flatMap((child, index) => moveTargets(child, `${path}.${index}`)),
    { id: `end:${path}`, place: "end", path, label: layoutText.end(tree.kind, path) },
  ];
}

function Node({ tree, path, scope }: { tree: LayoutTree; path: string; scope: Scope }): ReactNode {
  if (tree.kind === "widget") {
    return (
      // One wrapper in and out of edit mode, so toggling it never remounts the widget.
      <div className="omfp-view__item">
        {scope.onEdit !== undefined && (
          <WidgetToolbar
            reference={tree.name}
            path={path}
            scope={scope}
            onEdit={scope.onEdit}
            added={addedBy(scope.origins, path)}
          />
        )}
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
            {scope.onEdit !== undefined && <InitMark file={addedBy(scope.origins, id)} />}
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
 * as tabs (`omfp:file`), the selected one window-local too. One tab alone needs no tab bar.
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
  const ids = [...own.map(({ id }) => id), ...files.tabs.map((tab) => fileTabId(tab.path))];
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
  const activeFile = files.tabs.find((tab) => fileTabId(tab.path) === selected);
  return (
    <Tabs
      className="omfp-view__tabs"
      data-layout="tabs"
      data-path={path}
      data-slot={MAIN_SLOT}
      {...(selected === undefined ? {} : { selectedKey: selected })}
      onSelectionChange={(key) => files.select(String(key))}
    >
      <div className="omfp-view__tabbar">
        <TabList aria-label={text.views}>
          {[
            ...own.map(({ child, id }) => (
              <Tab key={id} id={id} data-testid="view-tab">
                {nodeTitle(child)}
                {scope.onEdit !== undefined && <InitMark file={addedBy(scope.origins, id)} />}
              </Tab>
            )),
            ...files.tabs.map((tab) => (
              <Tab key={fileTabId(tab.path)} id={fileTabId(tab.path)} data-testid="file-tab" data-file={tab.path}>
                <span title={tab.path}>{fileName(tab.path)}</span>
              </Tab>
            )),
          ]}
        </TabList>
        {activeFile !== undefined && (
          <IconButton
            label={filesText.close(fileName(activeFile.path))}
            icon={<Icon name="x" />}
            onPress={() => files.close(activeFile.path)}
            data-testid="file-tab-close"
          />
        )}
      </div>
      {own.map(({ child, id }) => (
        <TabPanel key={id} id={id} className="omfp-view__panel">
          <Node tree={child} path={id} scope={scope} />
        </TabPanel>
      ))}
      {files.tabs.map((tab) => (
        <TabPanel key={fileTabId(tab.path)} id={fileTabId(tab.path)} className="omfp-view__panel">
          <div className="omfp-view__item">
            <WidgetSlot
              reference={FILE_WIDGET}
              props={fileProps(tab)}
              path={fileTabId(tab.path)}
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
 * back to the view's sizes), or in edit mode writes the view's sizes. On a phone the cells of a row are tabs (all
 * mounted, one shown) and the view's root row shows the chat area or the side area (UI-204).
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
  const { onEdit, area } = scope;
  const key = sizesKey(scope.workspaceId, scope.viewId, path);
  const [own, setOwn] = useWindowState(PANE_SIZES);
  const count = tree.children.length;
  const sizes = effectiveSizes(tree.props.sizes, onEdit === undefined ? own[key] : undefined, count);
  // A pointer drag's sizes, shown until the new sizes arrive (the window's, or the daemon's push in edit mode).
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
    if (onEdit === undefined) {
      setPreview(undefined);
      setOwn((previous) => withSizes(previous, key, resizeSizes(current, index, delta)));
      return;
    }
    void onEdit({ kind: "resize", path, index, delta }).then((written) => {
      if (written) {
        // The window's own sizes would hide the new defaults once edit mode ends.
        setOwn((previous) => withSizes(previous, key, undefined));
      } else {
        setPreview(undefined);
      }
    });
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
                onReset={
                  onEdit === undefined
                    ? () => {
                        setPreview(undefined);
                        setOwn((previous) => withSizes(previous, key, undefined));
                      }
                    : undefined
                }
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

interface ToolbarProps {
  readonly reference: string;
  readonly path: string;
  readonly scope: Scope;
  readonly onEdit: (edit: LayoutEdit) => Promise<boolean>;
  /** The init file that added this widget (UI-269): it cannot move or hide, its toolbar names the file to edit. */
  readonly added: "user" | "workspace" | undefined;
}

/** The mark of a node an init file added (UI-269), in edit mode: names the file on hover and for screen readers. */
function InitMark({ file }: { file: "user" | "workspace" | undefined }): ReactNode {
  if (file === undefined) {
    return null;
  }
  const label = strings.init.edit.added(strings.init.files[file]);
  return (
    <span className="omfp-init-mark" title={label} data-testid="init-mark" data-file={file}>
      <span aria-hidden="true">{strings.init.edit.mark}</span>
      <span className="omfp-visually-hidden">{label}</span>
    </span>
  );
}

/**
 * A widget's edit-mode toolbar: its reference, Move (before/after another widget, end of a container) and Hide; for a
 * widget an init file added, its mark and the file to edit instead.
 */
function WidgetToolbar({ reference, path, scope, onEdit, added }: ToolbarProps): ReactNode {
  const targets = scope.targets.filter((target) => target.path !== path);
  if (added !== undefined) {
    const note = strings.init.edit.added(strings.init.files[added]);
    return (
      <Toolbar
        className="omfp-widget-toolbar"
        aria-label={layoutText.toolbar(reference)}
        data-testid="widget-toolbar"
        data-path={path}
        data-widget={reference}
        data-init={added}
      >
        <span className="omfp-widget-toolbar__reference">{reference}</span>
        <span className="omfp-init-mark" title={note} aria-hidden="true" data-testid="init-mark" data-file={added}>
          {strings.init.edit.mark}
        </span>
        <span className="omfp-widget-toolbar__note">{note}</span>
      </Toolbar>
    );
  }
  return (
    <Toolbar
      className="omfp-widget-toolbar"
      aria-label={layoutText.toolbar(reference)}
      data-testid="widget-toolbar"
      data-path={path}
      data-widget={reference}
    >
      <span className="omfp-widget-toolbar__reference">{reference}</span>
      <MenuTrigger>
        <Button size="sm" variant="quiet" aria-label={layoutText.moveLabel(reference)} data-testid="widget-move">
          {layoutText.move}
        </Button>
        <Popover placement="bottom start">
          <Menu
            aria-label={layoutText.moveLabel(reference)}
            items={targets}
            onAction={(key) => {
              const target = targets.find((candidate) => candidate.id === key);
              if (target !== undefined) {
                void onEdit({ kind: "move", from: path, place: target.place, to: target.path });
              }
            }}
          >
            {(target) => (
              <MenuItem id={target.id} data-testid="move-target" data-place={target.place} data-path={target.path}>
                {target.label}
              </MenuItem>
            )}
          </Menu>
        </Popover>
      </MenuTrigger>
      <Button
        size="sm"
        variant="quiet"
        aria-label={layoutText.hideLabel(reference)}
        data-testid="widget-hide"
        isDisabled={scope.single}
        onPress={(event) => {
          const toolbar = event.target.closest(TOOLBAR);
          const toolbars = [...(toolbar?.closest('[data-testid="view"]')?.querySelectorAll(TOOLBAR) ?? [])];
          scope.onHide?.(toolbar === null ? 0 : toolbars.indexOf(toolbar));
          void onEdit({ kind: "hide", path }).then((written) => {
            if (!written) {
              scope.onHide?.(undefined);
            }
          });
        }}
      >
        {layoutText.hide}
      </Button>
    </Toolbar>
  );
}

/** A tab's label: the widget's `title` prop, else a built-in's own name, else its reference; a container's kind. */
function nodeTitle(tree: LayoutTree): string {
  if (tree.kind !== "widget") {
    return tree.kind;
  }
  if (typeof tree.props.title === "string") {
    return tree.props.title;
  }
  return Object.hasOwn(text.widgetTitles, tree.name) ? (text.widgetTitles[tree.name] ?? tree.name) : tree.name;
}
