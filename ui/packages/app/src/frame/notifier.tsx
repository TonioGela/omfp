/**
 * Routes this window's notices (UI-096, UI-192, UI-194) as the daemon's `[ui] notifications` says (UI-097): with
 * `background` (default) a system notification while the window does not have focus, a mode-line notice while it
 * has; `always` adds the system notification while it has focus too; `never` shows neither. A notice about the
 * thread the user is on is dropped while the window has focus, unless its agent asked for it (`onThread`, UI-215);
 * visiting a thread clears its notices. Thread changes become notices here (`notifications.ts`), agents' requests in
 * `agent-notices.tsx`; widgets add theirs with `notify` (`sdk/notify.ts`).
 */

import type { ThreadSummary } from "@omfp/protocol";
import { type ReactNode, useEffect, useRef } from "react";
import {
  DEFAULT_NOTIFICATIONS,
  SELECTED_THREAD,
  useAnnounce,
  useHost,
  useSettings,
  useThreadActions,
  useThreads,
  useWindowState,
} from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { type NotificationPort, threadNotices } from "./notifications.ts";

export interface NotifierProps {
  readonly port: NotificationPort | undefined;
  /** `document.hasFocus()` in the browser. */
  readonly hasFocus: () => boolean;
  /** Brings the window to the front (`window.focus()`). */
  readonly focusWindow: () => void;
}

export function Notifier({ port, hasFocus, focusWindow }: NotifierProps): ReactNode {
  const host = useHost();
  const { notices, window } = host;
  const threads = useThreads();
  const actions = useThreadActions();
  const announce = useAnnounce();
  const mode = useSettings().ui.notifications ?? DEFAULT_NOTIFICATIONS;
  const [selected] = useWindowState(SELECTED_THREAD);
  const previous = useRef<ReadonlyMap<string, ThreadSummary> | undefined>(undefined);
  useAskPermission(mode === "never" ? undefined : port);
  useEffect(
    () =>
      notices.route((notice) => {
        if (mode === "never") {
          return;
        }
        const focused = hasFocus();
        if (
          focused &&
          notice.onThread !== true &&
          notice.threadId !== undefined &&
          notice.threadId === window.get(SELECTED_THREAD)
        ) {
          return;
        }
        const system =
          port !== undefined && port.permission() === "granted" && (mode === "always" || !focused) ? port : undefined;
        system?.show(notice, () => {
          focusWindow();
          if (notice.threadId !== undefined) {
            actions.select(notice.threadId);
          }
        });
        // Without focus the mode line keeps it only when no system notification could be shown.
        if (focused || system === undefined) {
          notices.show(notice);
          // The live region already speaks the threads awaiting input (UI-092).
          if (notice.tone !== "attention") {
            announce(strings.shell.notices.announce(notice.title, notice.body));
          }
        }
      }),
    [notices, window, port, mode, hasFocus, focusWindow, actions, announce],
  );
  useEffect(() => {
    const before = previous.current;
    previous.current = threads;
    if (before === undefined) {
      return;
    }
    for (const { tone, ...request } of threadNotices(before, threads)) {
      notices.notify(request, tone);
    }
  }, [threads, notices]);
  useEffect(() => {
    if (selected !== undefined) {
      notices.clearThread(selected);
    }
  }, [selected, notices]);
  return null;
}

/** Browsers only grant the permission from a user gesture: ask on the first key press or pointer press. */
function useAskPermission(port: NotificationPort | undefined): void {
  useEffect(() => {
    if (port === undefined || port.permission() !== "default") {
      return;
    }
    const ask = () => {
      stop();
      void port.requestPermission();
    };
    const stop = () => {
      document.removeEventListener("pointerdown", ask, true);
      document.removeEventListener("keydown", ask, true);
    };
    document.addEventListener("pointerdown", ask, true);
    document.addEventListener("keydown", ask, true);
    return stop;
  }, [port]);
}
