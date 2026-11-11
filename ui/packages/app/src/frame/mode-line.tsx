/**
 * The mode line (UI-098): the keymap mode (UI-095) the focused element puts the window in, shown at the bottom left
 * of the view area like vim's, and announced through the live region when a key changed it (never the mouse).
 */

import { ModePill, ModeSegment, type VimMode } from "@omfp/kit";
import { type ReactNode, useEffect, useState } from "react";
import { modeAt } from "../keymap/handler.ts";
import type { Mode } from "../keymap/keymap.ts";
import { SELECTED_THREAD, useAnnounce, useThreadSummary, useWindowState } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { displayTitle, statusLabel, threadStatus } from "../vm/threads.ts";

const text = strings.shell.mode;

/** This window's keymap mode, live; announces the changes a key caused. */
export function useMode(): Mode {
  const [mode, setMode] = useState<Mode>(() => modeAt(document.activeElement));
  const announce = useAnnounce();
  useEffect(() => {
    let byKey = false;
    let current = modeAt(document.activeElement);
    const onKey = () => {
      byKey = true;
    };
    const onPointer = () => {
      byKey = false;
    };
    const update = (focused: Element | null) => {
      const next = modeAt(focused);
      if (next === current) {
        return;
      }
      current = next;
      setMode(next);
      if (byKey) {
        announce(text.announce(text.names[next]));
      }
    };
    const onFocusIn = (event: FocusEvent) => update(event.target instanceof Element ? event.target : null);
    // While focus leaves, `relatedTarget` is where it goes (null: nowhere, the body).
    const onFocusOut = (event: FocusEvent) =>
      update(event.relatedTarget instanceof Element ? event.relatedTarget : null);
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("pointerdown", onPointer, true);
    document.addEventListener("mousedown", onPointer, true);
    document.addEventListener("focusin", onFocusIn);
    document.addEventListener("focusout", onFocusOut);
    return () => {
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("pointerdown", onPointer, true);
      document.removeEventListener("mousedown", onPointer, true);
      document.removeEventListener("focusin", onFocusIn);
      document.removeEventListener("focusout", onFocusOut);
    };
  }, [announce]);
  return mode;
}

/**
 * The bottom bar (UI-167): the mode pill — the keymap's mode, or vim's while a file editor has the focus (`vim`,
 * UI-112) — then the selected thread's title and state; in command mode `children` (the `:` line or the file picker)
 * replace the segments.
 */
export function ModeLine({
  mode,
  vim,
  children,
}: {
  mode: Mode;
  vim?: VimMode | undefined;
  children?: ReactNode;
}): ReactNode {
  const shown = vim === undefined ? mode : vim === "insert" || vim === "replace" ? "insert" : "normal";
  const pill = shown === "cmdline" ? "command" : shown;
  return (
    <div className="omfp-modeline omfp-frame__mode" data-testid="mode-bar" data-mode={pill}>
      <ModePill mode={pill} data-testid="mode-line" data-vim={vim}>
        {vim === undefined ? text.names[mode] : text.vim[vim]}
      </ModePill>
      <div className="omfp-modeline__segments">
        {mode !== "cmdline" && <ThreadSegments />}
        {children}
      </div>
    </div>
  );
}

/** The selected thread's title and state. */
function ThreadSegments(): ReactNode {
  const [selected] = useWindowState(SELECTED_THREAD);
  const summary = useThreadSummary(selected);
  if (summary === undefined) {
    return null;
  }
  return (
    <>
      <ModeSegment kind="title" data-testid="mode-thread-title">
        {displayTitle(summary)}
      </ModeSegment>
      <ModeSegment data-testid="mode-thread-state">{statusLabel(threadStatus(summary))}</ModeSegment>
    </>
  );
}
