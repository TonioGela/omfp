/**
 * The mode line (UI-098): the keymap mode (UI-095) the focused element puts the window in, shown at the bottom left
 * of the view area like vim's, and announced through the live region when a key changed it (never the mouse).
 */

import { type NoticeTone as KitTone, ModeNotice, ModePill, ModeSegment, type VimMode } from "@omfp/kit";
import { type ReactNode, useEffect, useState } from "react";
import { modeAt } from "../keymap/handler.ts";
import { type Binding, keyLabel, type Mode } from "../keymap/keymap.ts";
import {
  FOCUS_COMPOSER,
  NOTICES,
  type Notice,
  type NoticeTone,
  SELECTED_THREAD,
  useAnnounce,
  useCommand,
  useHost,
  useThreadActions,
  useThreadSummary,
  useWindowState,
} from "../sdk/index.ts";
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
 * UI-112) — then the selected thread's title and state, and the newest notice at the right end (UI-192); in command
 * mode `children` (the `:` line or the file picker) replace the segments and the notice.
 */
export function ModeLine({
  mode,
  vim,
  bindings,
  children,
}: {
  mode: Mode;
  vim?: VimMode | undefined;
  /** The window's bindings, for the key chip of `notice.open`. */
  bindings: readonly Binding[];
  children?: ReactNode;
}): ReactNode {
  const shown = vim === undefined ? mode : vim === "insert" || vim === "replace" ? "insert" : "normal";
  const pill = shown === "cmdline" ? "command" : shown;
  const open = useOpenNotice();
  return (
    <div className="omfp-modeline omfp-frame__mode" data-testid="mode-bar" data-mode={pill}>
      <ModePill mode={pill} data-testid="mode-line" data-vim={vim}>
        {vim === undefined ? text.names[mode] : text.vim[vim]}
      </ModePill>
      <div className="omfp-modeline__segments">
        {mode !== "cmdline" && <ThreadSegments />}
        {children}
      </div>
      {mode !== "cmdline" && <NoticeSlot bindings={bindings} open={open} />}
    </div>
  );
}

const KIT_TONES: Readonly<Record<NoticeTone, KitTone>> = {
  attention: "attention",
  failed: "error",
  finished: "ok",
  info: "info",
};

/**
 * Opens the newest notice (`notice.open`, `g n` by default, or a click on it): selects its thread, if any, with the
 * focus in its composer (UI-286), and drops it; visiting the thread clears the thread's other notices.
 */
function useOpenNotice(): () => void {
  const { notices, window } = useHost();
  const actions = useThreadActions();
  const open = () => {
    const newest: Notice | undefined = window.get(NOTICES).at(-1);
    if (newest === undefined) {
      return;
    }
    notices.dismiss(newest.id);
    if (newest.threadId !== undefined) {
      actions.select(newest.threadId);
      window.set(FOCUS_COMPOSER, newest.threadId);
    }
  };
  useCommand({
    id: "notice.open",
    title: strings.shell.notices.open,
    aliases: ["notice"],
    enabled: () => window.get(NOTICES).length > 0,
    run: open,
  });
  return open;
}

/** The newest notice and how many more wait behind it. */
function NoticeSlot({ bindings, open }: { bindings: readonly Binding[]; open: () => void }): ReactNode {
  const [shown] = useWindowState(NOTICES);
  const newest = shown.at(-1);
  if (newest === undefined) {
    return null;
  }
  const key = bindings.find((binding) => binding.mode === "normal" && binding.command === "notice.open")?.keys;
  return (
    <ModeNotice
      key={newest.id}
      data-testid="mode-notice"
      data-thread={newest.threadId}
      tone={KIT_TONES[newest.tone]}
      title={newest.title}
      body={newest.body}
      more={shown.length - 1}
      {...(key === undefined ? {} : { kbd: keyLabel(key) })}
      onPress={open}
    />
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
