/**
 * A panel floating over the view (the file panel, UI-115; the floating widgets, UI-230): its box and minimised state
 * in a window key, moved by a grip and resized by a corner (pointer drag, or arrow keys; Shift moves four times as far).
 */

import type { CSSProperties, KeyboardEvent, PointerEvent, RefObject } from "react";
import { useRef } from "react";
import { type FloatingPanel, type Update, useWindowState, type WindowKey } from "../sdk/index.ts";

/** How far an arrow key moves or resizes the panel; with Shift, four times as far. */
const KEY_STEP = 16;

type Box = NonNullable<FloatingPanel["box"]>;
export type Gesture = (panel: FloatingPanel, from: Box, dx: number, dy: number) => FloatingPanel;

/** The pointer and key handlers of a grip or a resize corner. */
export interface GestureHandlers {
  readonly onPointerDown: (event: PointerEvent<HTMLElement>) => void;
  readonly onPointerMove: (event: PointerEvent<HTMLElement>) => void;
  readonly onPointerUp: () => void;
  readonly onPointerCancel: () => void;
  readonly onKeyDown: (event: KeyboardEvent<HTMLElement>) => void;
}

export interface FloatingBox {
  readonly panel: FloatingPanel;
  readonly setPanel: (update: Update<FloatingPanel>) => void;
  /** The panel's element, measured when a gesture starts before the panel has a box of its own. */
  readonly root: RefObject<HTMLElement | null>;
  /** The handlers applying `gesture` (`movePanel`, `resizePanel`). */
  readonly handle: (gesture: Gesture) => GestureHandlers;
  /** The stored box as inline style; `undefined` while the stylesheet places the panel. */
  readonly style: CSSProperties | undefined;
}

export function useFloatingBox(key: WindowKey<FloatingPanel>): FloatingBox {
  const [panel, setPanel] = useWindowState(key);
  const root = useRef<HTMLElement>(null);
  const drag = useRef<{ readonly gesture: Gesture; readonly from: Box; readonly x: number; readonly y: number }>(
    undefined,
  );
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
  const handle = (gesture: Gesture): GestureHandlers => ({
    onPointerDown: (event) => {
      if (event.button !== 0) {
        return;
      }
      event.preventDefault();
      event.currentTarget.setPointerCapture?.(event.pointerId);
      drag.current = { gesture, from: currentBox(), x: event.clientX, y: event.clientY };
    },
    onPointerMove: (event) => {
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
    onKeyDown: (event) => {
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
  return { panel, setPanel, root, handle, style };
}
