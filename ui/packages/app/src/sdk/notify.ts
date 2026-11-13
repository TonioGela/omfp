/**
 * Notices (UI-096, UI-192, UI-194, UI-215): one way for the frame (a thread that starts needing the user, fails or
 * finishes a turn, or whose agent calls `omfp_notify`) and for widgets (`useNotify`) to tell the user something.
 * {@link Notices.notify} hands each notice to the route the frame attached: a system notification while the window has
 * no focus, else a notice in the mode line (`NOTICES`), never about the thread the user is on unless asked (`onThread`).
 * Notices in the mode line expire after {@link NOTICE_MS}; visiting their thread clears them. The window keeps its
 * last {@link MESSAGES_KEPT} notices and toasts with their time ({@link MESSAGES}, UI-521), which `:messages` lists.
 */

import { useCallback } from "react";
import { useHost } from "./host.tsx";
import { type WindowStore, windowKey } from "./window-store.ts";

/** A thread awaiting the user, a failed thread, a finished turn, or anything a widget says. */
export type NoticeTone = "attention" | "failed" | "finished" | "info";

/** What a widget passes to `notify`. */
export interface NoticeRequest {
  /** What it is about, e.g. a thread title. */
  readonly title: string;
  /** What happened. */
  readonly body: string;
  /** The thread it is about: opening the notice selects it, visiting it clears the notice. */
  readonly threadId?: string;
}

export interface Notice extends NoticeRequest {
  /** Unique in this window. */
  readonly id: number;
  readonly tone: NoticeTone;
  /** Shown even while the user is on its thread with focus: the thread's agent asked for it (UI-215). */
  readonly onThread?: boolean | undefined;
}

/** How the frame passes a notice on. */
export interface NoticeOptions {
  /** Show it even while the user is on its thread with focus (UI-215). */
  readonly onThread?: boolean | undefined;
}

/** The notices shown in this window's mode line, oldest first. */
export const NOTICES = windowKey<readonly Notice[]>("notices", []);

/** How long a notice stays in the mode line. */
export const NOTICE_MS = 8000;

/** What a line of the window's messages was: a notice's tone, or a success or error toast's. */
export type MessageTone = NoticeTone | "success" | "error";

/** A notice or a toast the window showed, with when (UI-521). */
export interface Message {
  /** Unique in this window. */
  readonly id: number;
  readonly tone: MessageTone;
  /** What it is about (a notice's title); toasts have none. */
  readonly title?: string;
  readonly text: string;
  /** When it showed, in milliseconds since the epoch. */
  readonly at: number;
}

/** This window's messages, oldest first: the notices and toasts it showed, the last {@link MESSAGES_KEPT}. */
export const MESSAGES = windowKey<readonly Message[]>("messages", []);

/** How many messages the window keeps. */
export const MESSAGES_KEPT = 100;

/** Where a notice goes; the frame attaches one (`frame/notifier.tsx`). */
export type NoticeRoute = (notice: Notice) => void;

/** The timers of notices and the clock of messages. */
export interface NoticeTimers {
  setTimeout(run: () => void, ms: number): void;
  now(): number;
}

const globalTimers: NoticeTimers = {
  setTimeout: (run, ms) => {
    globalThis.setTimeout(run, ms);
  },
  now: () => Date.now(),
};

export class Notices {
  readonly #window: WindowStore;
  readonly #timers: NoticeTimers;
  #route: NoticeRoute | undefined;
  #next = 1;
  #nextMessage = 1;

  constructor(window: WindowStore, timers: NoticeTimers = globalTimers) {
    this.#window = window;
    this.#timers = timers;
  }

  /** Routes notices (one route at a time); returns the detach. Without a route, notices go to the mode line. */
  route(route: NoticeRoute): () => void {
    this.#route = route;
    return () => {
      if (this.#route === route) {
        this.#route = undefined;
      }
    };
  }

  /** The one entry point: the frame's thread events, its agents' `omfp_notify` and widgets' `notify` all come here. */
  notify(request: NoticeRequest, tone: NoticeTone = "info", options: NoticeOptions = {}): void {
    const notice: Notice = { ...request, id: this.#next++, tone, ...(options.onThread === true && { onThread: true }) };
    if (this.#route === undefined) {
      this.show(notice);
    } else {
      this.#route(notice);
    }
  }

  /** Puts a notice in the mode line until it expires, is opened or its thread is visited; the messages keep it. */
  show(notice: Notice): void {
    this.#window.set(NOTICES, (notices) => [...notices, notice]);
    this.remember(notice.tone, notice.body, notice.title);
    this.#timers.setTimeout(() => this.dismiss(notice.id), NOTICE_MS);
  }

  /** Adds a line to the window's messages (UI-521), dropping the oldest beyond {@link MESSAGES_KEPT}. */
  remember(tone: MessageTone, text: string, title?: string): void {
    const message: Message = {
      id: this.#nextMessage++,
      tone,
      ...(title === undefined ? {} : { title }),
      text,
      at: this.#timers.now(),
    };
    this.#window.set(MESSAGES, (messages) => [...messages, message].slice(-MESSAGES_KEPT));
  }

  dismiss(id: number): void {
    this.#window.set(NOTICES, (notices) =>
      notices.some((notice) => notice.id === id) ? notices.filter((notice) => notice.id !== id) : notices,
    );
  }

  /** Clears the notices about a thread (the user is on it). */
  clearThread(threadId: string): void {
    this.#window.set(NOTICES, (notices) =>
      notices.some((notice) => notice.threadId === threadId)
        ? notices.filter((notice) => notice.threadId !== threadId)
        : notices,
    );
  }
}

/** `notify({ title, body, threadId? })`: a notice in this window's mode line, or a system notification while the
 * window has no focus (UI-194). */
export function useNotify(): (request: NoticeRequest) => void {
  const { notices } = useHost();
  return useCallback((request: NoticeRequest) => notices.notify(request), [notices]);
}
