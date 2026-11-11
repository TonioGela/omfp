/**
 * Thread notices (UI-096, UI-192): a thread that starts needing the user (an approval or `ask` — `needsAttention` —,
 * or a failed process) or finishes a turn (it settles). They go through `host.notices` like a widget's `notify`:
 * a system notification while the window has no focus, else a notice in the mode line. The Notification API is
 * injected (a {@link NotificationPort}) so tests drive it; permission is asked on the first user gesture.
 */

import type { ThreadSummary } from "@omfp/protocol";
import type { Notice, NoticeTone } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { displayTitle, isArchived } from "../vm/threads.ts";

export type ThreadNoticeTone = Exclude<NoticeTone, "info">;

export interface ThreadNotice {
  readonly threadId: string;
  readonly tone: ThreadNoticeTone;
  readonly title: string;
  readonly body: string;
}

/** What changed from `previous` to `next` that deserves a notice; threads new to `next` have no baseline. */
export function threadNotices(
  previous: ReadonlyMap<string, ThreadSummary>,
  next: ReadonlyMap<string, ThreadSummary>,
): ThreadNotice[] {
  const notices: ThreadNotice[] = [];
  for (const thread of next.values()) {
    const before = previous.get(thread.id);
    const tone = before === undefined || isArchived(thread) ? undefined : change(before, thread);
    if (tone !== undefined) {
      notices.push({ threadId: thread.id, tone, title: displayTitle(thread), body: strings.shell.notices[tone] });
    }
  }
  return notices;
}

function change(before: ThreadSummary, after: ThreadSummary): ThreadNoticeTone | undefined {
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
  /** Shows one system notification; `onClick` runs when the user clicks it. */
  show(notice: Notice, onClick: () => void): void;
}

/** The browser's Notification API; `undefined` where there is none. */
export function browserNotifications(): NotificationPort | undefined {
  if (typeof Notification === "undefined") {
    return undefined;
  }
  return {
    permission: () => Notification.permission,
    requestPermission: () => Notification.requestPermission(),
    show: (notice, onClick) => {
      const shown = new Notification(notice.title, {
        body: notice.body,
        tag: notice.threadId ?? `notice-${notice.id}`,
      });
      shown.onclick = () => {
        onClick();
        shown.close();
      };
    },
  };
}
