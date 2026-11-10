/**
 * Shows system notifications (UI-096) as the daemon's `[ui] notifications` says (UI-097): `background` (default) only
 * while the window does not have focus, `always`, or `never`; see `notifications.ts`.
 */

import type { ThreadSummary } from "@omfp/protocol";
import { type ReactNode, useEffect, useRef } from "react";
import { DEFAULT_NOTIFICATIONS, useSettings, useThreadActions, useThreads } from "../sdk/index.ts";
import { type NotificationPort, notificationsFor } from "./notifications.ts";

export interface NotifierProps {
  readonly port: NotificationPort | undefined;
  /** `document.hasFocus()` in the browser. */
  readonly hasFocus: () => boolean;
  /** Brings the window to the front (`window.focus()`). */
  readonly focusWindow: () => void;
}

export function Notifier({ port, hasFocus, focusWindow }: NotifierProps): ReactNode {
  const threads = useThreads();
  const actions = useThreadActions();
  const mode = useSettings().ui.notifications ?? DEFAULT_NOTIFICATIONS;
  const previous = useRef<ReadonlyMap<string, ThreadSummary> | undefined>(undefined);
  useAskPermission(mode === "never" ? undefined : port);
  useEffect(() => {
    const before = previous.current;
    previous.current = threads;
    const muted = mode === "never" || (mode === "background" && hasFocus());
    if (before === undefined || port === undefined || muted || port.permission() !== "granted") {
      return;
    }
    for (const notification of notificationsFor(before, threads)) {
      port.show(notification, () => {
        focusWindow();
        actions.select(notification.threadId);
      });
    }
  }, [threads, port, mode, hasFocus, focusWindow, actions]);
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
