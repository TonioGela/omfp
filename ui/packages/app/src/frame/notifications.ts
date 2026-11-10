/**
 * System notifications (UI-096): when a thread starts needing the user (an approval or `ask` — `needsAttention` —, or
 * a failed process) or finishes a turn (it settles), and the window does not have focus. The Notification API is
 * injected (a {@link NotificationPort}) so tests drive it; permission is asked on the first user gesture.
 */

import type { ThreadSummary } from "@omfp/protocol";
import { strings } from "../strings/index.ts";
import { displayTitle, isArchived } from "../vm/threads.ts";

export type NotificationKind = "attention" | "failed" | "finished";

export interface ThreadNotification {
  readonly threadId: string;
  readonly kind: NotificationKind;
  readonly title: string;
  readonly body: string;
}

/** What changed from `previous` to `next` that deserves a notification; threads new to `next` have no baseline. */
export function notificationsFor(
  previous: ReadonlyMap<string, ThreadSummary>,
  next: ReadonlyMap<string, ThreadSummary>,
): ThreadNotification[] {
  const notifications: ThreadNotification[] = [];
  for (const thread of next.values()) {
    const before = previous.get(thread.id);
    const kind = before === undefined || isArchived(thread) ? undefined : change(before, thread);
    if (kind !== undefined) {
      notifications.push({
        threadId: thread.id,
        kind,
        title: displayTitle(thread),
        body: strings.shell.notifications[kind],
      });
    }
  }
  return notifications;
}

function change(before: ThreadSummary, after: ThreadSummary): NotificationKind | undefined {
  if (after.needsAttention && !before.needsAttention) {
    return "attention";
  }
  if (after.process.state === "failed" && before.process.state !== "failed") {
    return "failed";
  }
  if (after.settled && !before.settled && after.process.state === "running" && !after.needsAttention) {
    return "finished";
  }
  return undefined;
}

/** The Notification API as the frame uses it. */
export interface NotificationPort {
  permission(): NotificationPermission;
  requestPermission(): Promise<NotificationPermission>;
  /** Shows one notification; `onClick` runs when the user clicks it. */
  show(notification: ThreadNotification, onClick: () => void): void;
}

/** The browser's Notification API; `undefined` where there is none. */
export function browserNotifications(): NotificationPort | undefined {
  if (typeof Notification === "undefined") {
    return undefined;
  }
  return {
    permission: () => Notification.permission,
    requestPermission: () => Notification.requestPermission(),
    show: (notification, onClick) => {
      const shown = new Notification(notification.title, { body: notification.body, tag: notification.threadId });
      shown.onclick = () => {
        onClick();
        shown.close();
      };
    },
  };
}
