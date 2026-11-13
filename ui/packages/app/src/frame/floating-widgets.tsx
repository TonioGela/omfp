/**
 * Floating panels (UI-230, AGT-060): each panel of the current workspace's valid manifest floats over whatever view the
 * window shows, with a bar (grip, title, minimise, close) over its node and a resize corner. Panels start cascaded;
 * each keeps its box and minimised state per workspace and id in this window. Closing one removes it from
 * `views.json` with `views.unmount` (UI-231); closing one an init file added hides it in this window (UI-261).
 */

import { IconButton } from "@omfp/kit";
import type { ViewsState } from "@omfp/protocol";
import { type CSSProperties, type ReactNode, useEffect } from "react";
import {
  floatingWidgetKey,
  movePanel,
  NO_FILE_TABS,
  OPEN_FILES,
  openWidgetTab,
  REVEAL_WIDGET,
  reportError,
  resizePanel,
  useHost,
  useWindowState,
} from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { containsWidget, hasMainSlot } from "../view/layout.ts";
import { PanelLayout, widgetTitle } from "../view/render.tsx";
import type { WidgetRegistry } from "../view/resolve.ts";
import type { ResolvedPanel, ResolvedView } from "../view/views.ts";
import { useFloatingBox } from "./floating-box.ts";

const text = strings.shell.panels;

export interface FloatingWidgetsProps {
  readonly panels: readonly ResolvedPanel[];
  readonly workspaceId: string | undefined;
  readonly views: ViewsState | undefined;
  readonly registry: WidgetRegistry;
}

export function FloatingWidgets({ panels, workspaceId, views, registry }: FloatingWidgetsProps): ReactNode {
  if (workspaceId === undefined) {
    return null;
  }
  return panels.map((panel, index) => (
    <FloatingWidget
      key={panel.id}
      panel={panel}
      index={index}
      workspaceId={workspaceId}
      views={views}
      registry={registry}
    />
  ));
}

export interface RevealedWidgetProps extends FloatingWidgetsProps {
  readonly view: ResolvedView | undefined;
}

/**
 * The widget of the window's latest reveal request (UI-274, UI-373) when neither the view nor a panel of the manifest
 * holds it: a closable tab of the view's `main` slot (with the open files, window-local per workspace, UI-373), else,
 * in a view without a `main` slot, a floating panel of this window only (`views.json` is not written); closing it
 * drops the request.
 */
export function RevealedWidget({ view, panels, workspaceId, views, registry }: RevealedWidgetProps): ReactNode {
  const [request, setRequest] = useWindowState(REVEAL_WIDGET);
  const [, setOpened] = useWindowState(OPEN_FILES);
  const names = request === undefined ? [] : [request.reference];
  const held =
    (view !== undefined && containsWidget(view.tree, names)) ||
    panels.some((panel) => containsWidget(panel.tree, names));
  const tab = !held && workspaceId !== undefined && view !== undefined && hasMainSlot(view.tree);
  // Once the view's tabs showed it (their effects run first) or a tab of `main` opened it, the request is done: a later
  // view without the widget must not float it.
  useEffect(() => {
    if (request === undefined) {
      return;
    }
    if (tab && workspaceId !== undefined) {
      setOpened((all) => ({
        ...all,
        [workspaceId]: openWidgetTab(all[workspaceId] ?? NO_FILE_TABS, request.reference),
      }));
      setRequest(undefined);
    } else if (held) {
      setRequest(undefined);
    }
  }, [request, held, tab, workspaceId, setOpened, setRequest]);
  if (request === undefined || workspaceId === undefined || held || tab) {
    return null;
  }
  const panel: ResolvedPanel = {
    id: `reveal/${request.reference}`,
    title: widgetTitle(request.reference),
    node: request.reference,
    tree: { kind: "widget", name: request.reference, props: {} },
  };
  return (
    <FloatingWidget
      panel={panel}
      index={panels.length}
      workspaceId={workspaceId}
      views={views}
      registry={registry}
      onClose={() => setRequest(undefined)}
    />
  );
}

interface FloatingWidgetProps {
  readonly panel: ResolvedPanel;
  /** Closes the panel; by default `views.unmount` removes it from `views.json` (UI-231). */
  readonly onClose?: () => void;
  /** Its place in id order: panels without a box of their own are cascaded by it. */
  readonly index: number;
  readonly workspaceId: string;
  readonly views: ViewsState | undefined;
  readonly registry: WidgetRegistry;
}

function FloatingWidget({ panel, index, workspaceId, views, registry, onClose }: FloatingWidgetProps): ReactNode {
  const host = useHost();
  const { panel: state, setPanel, root, handle, style } = useFloatingBox(floatingWidgetKey(workspaceId, panel.id));
  const { title } = panel;
  const close = () => {
    if (onClose !== undefined) {
      onClose();
      return;
    }
    if (panel.init !== undefined) {
      host.init.closePanel(panel.id);
      return;
    }
    host.client
      .call("views.unmount", { workspaceId, id: panel.id })
      .catch((error: unknown) => reportError(host, error));
  };
  return (
    <section
      ref={root}
      className="omfp-floating-widget"
      aria-label={title}
      data-testid="floating-widget"
      data-panel={panel.id}
      data-minimised={state.minimised ? "" : undefined}
      style={style ?? ({ "--panel-index": index } as CSSProperties)}
    >
      <div className="omfp-floating-file__bar">
        <button
          type="button"
          className="omfp-floating-file__grip"
          aria-label={text.move(title)}
          title={text.move(title)}
          data-testid="floating-widget-move"
          {...handle(movePanel)}
        >
          ⠿
        </button>
        <span className="omfp-floating-file__title" data-testid="floating-widget-title">
          {title}
        </span>
        <IconButton
          label={state.minimised ? text.restore(title) : text.minimise(title)}
          icon={state.minimised ? "□" : "–"}
          onPress={() => setPanel((now) => ({ ...now, minimised: !now.minimised }))}
          data-testid="floating-widget-minimise"
        />
        <IconButton label={text.close(title)} icon="×" onPress={close} data-testid="floating-widget-close" />
      </div>
      {/* Minimised, the widget stays mounted (its state too), hidden. */}
      <div className="omfp-floating-file__body" hidden={state.minimised}>
        <PanelLayout id={panel.id} tree={panel.tree} registry={registry} workspaceId={workspaceId} views={views} />
      </div>
      {!state.minimised && (
        <button
          type="button"
          className="omfp-floating-file__resize"
          aria-label={text.resize(title)}
          title={text.resize(title)}
          data-testid="floating-widget-resize"
          {...handle(resizePanel)}
        />
      )}
    </section>
  );
}
