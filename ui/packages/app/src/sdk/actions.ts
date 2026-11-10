/**
 * Thread actions shared by widgets, commands and the frame (UI-026, UI-030): each is one protocol call plus what the
 * window must forget. Failures land in the {@link ERRORS} slot the frame shows.
 */

import { useMemo } from "react";
import { type OmfpHost, useHost } from "./host.tsx";
import { DELETE_THREAD, draftKey, ERRORS, RENAMING_THREAD, SELECTED_THREAD } from "./keys.ts";

export interface ThreadActions {
  /** Shows `threadId` in this window (`undefined`: none). */
  select(threadId: string | undefined): void;
  archive(threadId: string): Promise<void>;
  unarchive(threadId: string): Promise<void>;
  /** Opens the delete confirmation; {@link ThreadActions.remove} is what it confirms. */
  requestDelete(threadId: string): void;
  /** Deletes without asking. */
  remove(threadId: string): Promise<void>;
  rename(threadId: string, title: string): Promise<void>;
  setTags(threadId: string, tags: readonly string[]): Promise<void>;
  stop(threadId: string): Promise<void>;
  restart(threadId: string): Promise<void>;
}

/** The message of a failed call or any thrown value. */
export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Adds `error` to the window's error list. */
export function reportError(host: OmfpHost, error: unknown): void {
  host.window.set(ERRORS, (errors) => [...errors, errorMessage(error)]);
}

export function useReportError(): (error: unknown) => void {
  const host = useHost();
  return useMemo(() => (error: unknown) => reportError(host, error), [host]);
}

/** Drops what the window keeps for a thread that is archived or deleted: selection, draft, open editors (UI-026). */
function forget(host: OmfpHost, threadId: string): void {
  const { window } = host;
  for (const key of [SELECTED_THREAD, RENAMING_THREAD, DELETE_THREAD]) {
    if (window.get(key) === threadId) {
      window.set(key, undefined);
    }
  }
  window.reset(draftKey(threadId));
}

export function threadActions(host: OmfpHost): ThreadActions {
  const { client, window } = host;
  const attempt = async (call: () => Promise<unknown>): Promise<void> => {
    try {
      await call();
    } catch (error) {
      reportError(host, error);
    }
  };
  return {
    select: (threadId) => window.set(SELECTED_THREAD, threadId),
    archive: (threadId) => {
      forget(host, threadId);
      return attempt(() => client.call("threads.archive", { threadId }));
    },
    unarchive: (threadId) => attempt(() => client.call("threads.unarchive", { threadId })),
    requestDelete: (threadId) => window.set(DELETE_THREAD, threadId),
    remove: (threadId) => {
      forget(host, threadId);
      return attempt(() => client.call("threads.delete", { threadId }));
    },
    rename: (threadId, title) => attempt(() => client.call("threads.update", { threadId, title })),
    setTags: (threadId, tags) => attempt(() => client.call("threads.update", { threadId, tags: [...tags] })),
    stop: (threadId) => attempt(() => client.call("thread.stop", { threadId })),
    restart: (threadId) => attempt(() => client.call("thread.restart", { threadId })),
  };
}

/** {@link ThreadActions} of this window. */
export function useThreadActions(): ThreadActions {
  const host = useHost();
  return useMemo(() => threadActions(host), [host]);
}
