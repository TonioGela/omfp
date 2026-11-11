/**
 * Renders a validated view (AGT-010): containers become flex rows and columns, tabs or stacked layers; each widget
 * renders in its own slot, so a failing widget shows its error in place and the rest of the window keeps working. In
 * this window's edit mode (UI-103) every widget gets a toolbar (reference, Move, Hide) and the cells of each `row` and
 * `col` get splitters; each change goes to `onEdit`, and the view then renders what the daemon pushes back.
 */

import { Button, Menu, MenuItem, MenuTrigger, Popover, Tab, TabList, TabPanel, Tabs, Toolbar } from "@omfp/kit";
import type { ViewsState } from "@omfp/protocol";
import { Fragment, type KeyboardEvent, type PointerEvent, type ReactNode, useMemo, useRef, useState } from "react";
import { strings } from "../strings/index.ts";
import { type LayoutEdit, type Place, RESIZE_STEP, resizeSizes } from "./edit.ts";
import type { LayoutTree } from "./layout.ts";
import type { WidgetRegistry } from "./resolve.ts";
import { WidgetSlot } from "./slot.tsx";
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
  /** Edit mode (UI-103): writes a layout edit, resolving to whether it was written; absent outside edit mode. */
  readonly onEdit?: ((edit: LayoutEdit) => Promise<boolean>) | undefined;
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
}

export function ViewRenderer({ view, registry, workspaceId, views, onEdit }: ViewRendererProps): ReactNode {
  const targets = useMemo(() => moveTargets(view.tree, "0"), [view.tree]);
  const scope: Scope = {
    registry,
    workspaceId,
    views,
    onEdit,
    targets,
    single: view.tree.kind === "widget",
  };
  return (
    <div className="omfp-view" data-testid="view" data-view={view.id} data-editing={onEdit !== undefined}>
      <Node tree={view.tree} path="0" scope={scope} />
    </div>
  );
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
          <WidgetToolbar reference={tree.name} path={path} scope={scope} onEdit={scope.onEdit} />
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
    const children = tree.children.map((child, index) => ({ child, path: `${path}.${index}` }));
    return (
      <Tabs className="omfp-view__tabs" data-layout="tabs" data-path={path}>
        <TabList aria-label={text.views}>
          {children.map(({ child, path: id }) => (
            <Tab key={id} id={id} data-testid="view-tab">
              {nodeTitle(child)}
            </Tab>
          ))}
        </TabList>
        {children.map(({ child, path: id }) => (
          <TabPanel key={id} id={id} className="omfp-view__panel">
            <Node tree={child} path={id} scope={scope} />
          </TabPanel>
        ))}
      </Tabs>
    );
  }
  return <Cells tree={tree} path={path} scope={scope} />;
}

/** A `row`, `col` or `stack`: its children side by side, stacked or layered; splitters between them in edit mode. */
function Cells({
  tree,
  path,
  scope,
}: {
  tree: Exclude<LayoutTree, { kind: "widget" }>;
  path: string;
  scope: Scope;
}): ReactNode {
  const { sizes } = tree.props;
  // A pointer drag's sizes, shown until the daemon pushes the written ones (or the write fails).
  const [preview, setPreview] = useState<{ base: readonly number[] | undefined; sizes: readonly number[] }>();
  const shown = preview !== undefined && preview.base === sizes ? preview.sizes : sizes;
  const { onEdit } = scope;
  const resizable = onEdit !== undefined && (tree.kind === "row" || tree.kind === "col");
  const written = sizes ?? tree.children.map(() => 1);
  return (
    <div className={`omfp-view__${tree.kind}`} data-layout={tree.kind} data-path={path}>
      {tree.children.map((child, index) => {
        const id = `${path}.${index}`;
        return (
          <Fragment key={id}>
            {resizable && index > 0 && (
              <Splitter
                kind={tree.kind === "row" ? "row" : "col"}
                path={path}
                index={index}
                weights={shown ?? written}
                onPreview={(delta) =>
                  setPreview(
                    delta === undefined ? undefined : { base: sizes, sizes: resizeSizes(written, index, delta) },
                  )
                }
                onResize={(delta) => {
                  void onEdit({ kind: "resize", path, index, delta }).then((written) => {
                    if (!written) {
                      setPreview(undefined);
                    }
                  });
                }}
              />
            )}
            <div className="omfp-view__cell" style={shown === undefined ? undefined : { flexGrow: shown[index] }}>
              <Node tree={child} path={id} scope={scope} />
            </div>
          </Fragment>
        );
      })}
    </div>
  );
}

interface SplitterProps {
  readonly kind: "row" | "col";
  readonly path: string;
  /** Index of the cell after the splitter. */
  readonly index: number;
  readonly weights: readonly number[];
  /** A drag in progress (`undefined`: none). */
  readonly onPreview: (delta: number | undefined) => void;
  /** A finished change: `delta` is the share of the container moved to the cell before the splitter. */
  readonly onResize: (delta: number) => void;
}

/** Resizes two adjacent cells with the pointer or the arrow keys (5 % steps, UI-103). */
function Splitter({ kind, path, index, weights, onPreview, onResize }: SplitterProps): ReactNode {
  const drag = useRef<{ readonly start: number; readonly length: number }>(undefined);
  const before = weights[index - 1] ?? 1;
  const after = weights[index] ?? 1;
  const position = (event: PointerEvent) => (kind === "row" ? event.clientX : event.clientY);
  const delta = (event: PointerEvent) =>
    drag.current === undefined ? 0 : (position(event) - drag.current.start) / drag.current.length;
  return (
    // biome-ignore lint/a11y/useSemanticElements: a focusable, adjustable separator (WAI-ARIA window splitter); <hr> cannot take focus or keys.
    <div
      className="omfp-splitter"
      role="separator"
      tabIndex={0}
      aria-orientation={kind === "row" ? "vertical" : "horizontal"}
      aria-label={layoutText.splitter(kind, path, index)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round((before / (before + after)) * 100)}
      data-testid="splitter"
      data-path={path}
      data-index={index}
      onKeyDown={(event: KeyboardEvent) => {
        const step = { ArrowLeft: -1, ArrowUp: -1, ArrowRight: 1, ArrowDown: 1 }[event.key];
        if (step !== undefined) {
          event.preventDefault();
          onResize(step * RESIZE_STEP);
        }
      }}
      onPointerDown={(event) => {
        const box = event.currentTarget.parentElement?.getBoundingClientRect();
        const length = box === undefined ? 0 : kind === "row" ? box.width : box.height;
        if (length > 0) {
          event.currentTarget.setPointerCapture(event.pointerId);
          drag.current = { start: position(event), length };
        }
      }}
      onPointerMove={(event) => {
        if (drag.current !== undefined) {
          onPreview(delta(event));
        }
      }}
      onPointerUp={(event) => {
        const moved = delta(event);
        drag.current = undefined;
        if (moved === 0) {
          onPreview(undefined);
        } else {
          onPreview(moved);
          onResize(moved);
        }
      }}
      onPointerCancel={() => {
        drag.current = undefined;
        onPreview(undefined);
      }}
    />
  );
}

interface ToolbarProps {
  readonly reference: string;
  readonly path: string;
  readonly scope: Scope;
  readonly onEdit: (edit: LayoutEdit) => Promise<boolean>;
}

/** A widget's edit-mode toolbar: its reference, Move (before/after another widget, end of a container) and Hide. */
function WidgetToolbar({ reference, path, scope, onEdit }: ToolbarProps): ReactNode {
  const targets = scope.targets.filter((target) => target.path !== path);
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
        onPress={() => {
          void onEdit({ kind: "hide", path });
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
