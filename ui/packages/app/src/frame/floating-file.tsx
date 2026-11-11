/**
 * Files in a view without a `main` slot (UI-115): a panel floating over the view holding the open files as tabs, like
 * the `main` slot's (selected tab, closing selects the neighbour, window-local). The panel moves by its grip and
 * resizes by its corner (pointer or arrow keys), minimises to its bar, and keeps its place and state in this window.
 * Esc closes the selected file (an Esc the file editor kept for itself does not count).
 */

import { IconButton, Tab, TabList, TabPanel, Tabs } from "@omfp/kit";
import type { ViewsState } from "@omfp/protocol";
import type { KeyboardEvent, PointerEvent, ReactNode } from "react";
import { useRef } from "react";
import {
  FLOATING_FILES,
  type FloatingPanel,
  fileName,
  fileTabId,
  movePanel,
  resizePanel,
  useFileTabs,
  useWindowState,
} from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { FILE_WIDGET, fileProps } from "../view/layout.ts";
import type { WidgetRegistry } from "../view/resolve.ts";
import { WidgetSlot } from "../view/slot.tsx";

const text = strings.shell.files;

/** How far an arrow key moves or resizes the panel; with Shift, four times as far. */
const KEY_STEP = 16;

type Box = NonNullable<FloatingPanel["box"]>;
type Gesture = (panel: FloatingPanel, from: Box, dx: number, dy: number) => FloatingPanel;

export interface FloatingFileProps {
  readonly workspaceId: string | undefined;
  readonly views: ViewsState | undefined;
  readonly registry: WidgetRegistry;
}

export function FloatingFile({ workspaceId, views, registry }: FloatingFileProps): ReactNode {
  const files = useFileTabs(workspaceId);
  const [panel, setPanel] = useWindowState(FLOATING_FILES);
  const root = useRef<HTMLElement>(null);
  const drag = useRef<{ readonly gesture: Gesture; readonly from: Box; readonly x: number; readonly y: number }>(
    undefined,
  );
  const first = files.tabs[0];
  if (first === undefined) {
    return null;
  }
  const selected = files.tabs.find((tab) => fileTabId(tab.path) === files.active) ?? first;
  /** The panel's box now: the stored one, else where the stylesheet put it. */
  const currentBox = (): Box => {
    const element = root.current;
    return (
      panel.box ?? {
        left: element?.offsetLeft ?? 0,
        top: element?.offsetTop ?? 0,
        width: element?.offsetWidth ?? 0,
        height: element?.offsetHeight ?? 0,
      }
    );
  };
  const handle = (gesture: Gesture) => ({
    onPointerDown: (event: PointerEvent<HTMLElement>) => {
      if (event.button !== 0) {
        return;
      }
      event.preventDefault();
      event.currentTarget.setPointerCapture?.(event.pointerId);
      drag.current = { gesture, from: currentBox(), x: event.clientX, y: event.clientY };
    },
    onPointerMove: (event: PointerEvent<HTMLElement>) => {
      const started = drag.current;
      if (started !== undefined) {
        setPanel((now) => started.gesture(now, started.from, event.clientX - started.x, event.clientY - started.y));
      }
    },
    onPointerUp: () => {
      drag.current = undefined;
    },
    onPointerCancel: () => {
      drag.current = undefined;
    },
    onKeyDown: (event: KeyboardEvent<HTMLElement>) => {
      const step = event.shiftKey ? KEY_STEP * 4 : KEY_STEP;
      const delta = (
        {
          ArrowLeft: [-step, 0],
          ArrowRight: [step, 0],
          ArrowUp: [0, -step],
          ArrowDown: [0, step],
        } as Readonly<Record<string, readonly [number, number]>>
      )[event.key];
      if (delta !== undefined) {
        event.preventDefault();
        event.stopPropagation();
        setPanel((now) => gesture(now, currentBox(), delta[0], delta[1]));
      }
    },
  });
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && !event.defaultPrevented) {
      event.preventDefault();
      files.close(selected.path);
    }
  };
  const style =
    panel.box === undefined
      ? undefined
      : {
          left: panel.box.left,
          top: panel.box.top,
          width: panel.box.width,
          height: panel.minimised ? undefined : panel.box.height,
          right: "auto",
          bottom: "auto",
        };
  const bar = (tabs: ReactNode) => (
    <div className="omfp-floating-file__bar">
      <button
        type="button"
        className="omfp-floating-file__grip"
        aria-label={text.move}
        title={text.move}
        data-testid="floating-file-move"
        {...handle(movePanel)}
      >
        ⠿
      </button>
      {tabs}
      <IconButton
        label={panel.minimised ? text.restore : text.minimise}
        icon={panel.minimised ? "□" : "–"}
        onPress={() => setPanel((now) => ({ ...now, minimised: !now.minimised }))}
        data-testid="floating-file-minimise"
      />
      <IconButton
        label={text.close(fileName(selected.path))}
        icon="×"
        onPress={() => files.close(selected.path)}
        data-testid="floating-file-close"
      />
    </div>
  );
  return (
    <section
      ref={root}
      className="omfp-floating-file"
      aria-label={text.panel(files.tabs.length)}
      data-testid="floating-file"
      data-minimised={panel.minimised ? "" : undefined}
      style={style}
      onKeyDown={onKeyDown}
    >
      {panel.minimised ? (
        bar(<span className="omfp-floating-file__title">{text.panel(files.tabs.length)}</span>)
      ) : (
        <>
          <Tabs
            className="omfp-floating-file__tabs"
            selectedKey={fileTabId(selected.path)}
            onSelectionChange={(key) => files.select(String(key))}
          >
            {bar(
              <TabList aria-label={text.tabs} className="omfp-floating-file__list">
                {files.tabs.map((tab) => (
                  <Tab
                    key={fileTabId(tab.path)}
                    id={fileTabId(tab.path)}
                    data-testid="floating-file-tab"
                    data-file={tab.path}
                  >
                    <span title={tab.path}>{fileName(tab.path)}</span>
                  </Tab>
                ))}
              </TabList>,
            )}
            {files.tabs.map((tab) => (
              <TabPanel key={fileTabId(tab.path)} id={fileTabId(tab.path)} className="omfp-floating-file__body">
                <WidgetSlot
                  reference={FILE_WIDGET}
                  props={fileProps(tab)}
                  path={fileTabId(tab.path)}
                  workspaceId={workspaceId}
                  views={views}
                  registry={registry}
                />
              </TabPanel>
            ))}
          </Tabs>
          <button
            type="button"
            className="omfp-floating-file__resize"
            aria-label={text.resize}
            title={text.resize}
            data-testid="floating-file-resize"
            {...handle(resizePanel)}
          />
        </>
      )}
    </section>
  );
}
