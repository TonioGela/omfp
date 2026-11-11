/**
 * Floating panels (UI-230, AGT-060): each panel of the current workspace's valid manifest floats over whatever view the
 * window shows, with a bar (grip, title, minimise, close) over its node and a resize corner. Panels start cascaded;
 * each keeps its box and minimised state per workspace and id in this window. Closing one removes it from
 * `views.json` with `views.unmount` (UI-231).
 */

import { IconButton } from "@omfp/kit";
import type { ViewsState } from "@omfp/protocol";
import type { CSSProperties, ReactNode } from "react";
import { floatingWidgetKey, movePanel, reportError, resizePanel, useHost } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { PanelLayout } from "../view/render.tsx";
import type { WidgetRegistry } from "../view/resolve.ts";
import type { ResolvedPanel } from "../view/views.ts";
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

interface FloatingWidgetProps {
  readonly panel: ResolvedPanel;
  /** Its place in id order: panels without a box of their own are cascaded by it. */
  readonly index: number;
  readonly workspaceId: string;
  readonly views: ViewsState | undefined;
  readonly registry: WidgetRegistry;
}

function FloatingWidget({ panel, index, workspaceId, views, registry }: FloatingWidgetProps): ReactNode {
  const host = useHost();
  const { panel: state, setPanel, root, handle, style } = useFloatingBox(floatingWidgetKey(workspaceId, panel.id));
  const { title } = panel;
  const close = () => {
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
