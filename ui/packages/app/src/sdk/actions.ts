/**
 * Thread actions shared by widgets, commands and the frame (UI-023, UI-026, UI-030): each is one protocol call plus
 * what the window must forget. Failures land in the {@link ERRORS} slot the frame shows.
 */

import { useMemo } from "react";
import { currentWorkspaceId as workspaceOf } from "../vm/workspaces.ts";
import { useThreadSummary, useWindowState } from "./hooks.ts";
import { type OmfpHost, useHost } from "./host.tsx";
import {
  DELETE_THREAD,
  DRAFT_MESSAGE,
  DRAFT_THREAD,
  draftKey,
  ERRORS,
  RENAMING_THREAD,
  SELECTED_THREAD,
} from "./keys.ts";

export interface ThreadActions {
  /** Shows `threadId` in this window (`undefined`: none); leaving an empty draft thread discards it (UI-023). */
  select(threadId: string | undefined): void;
  /** Shows the draft thread of `workspaceId` (UI-023); a draft started in another workspace moves there with its
   * text. */
  openDraft(workspaceId: string): void;
  /** Drops the draft thread and its text. */
  discardDraft(): void;
  /** Creates the daemon thread of the draft with its text as first message (`threads.create { workspace, prompt }`),
   * then selects it and drops the draft; on failure the draft and its text stay. Resolves to whether it worked. */
  sendDraft(): Promise<boolean>;
  archive(threadId: string): Promise<void>;
  unarchive(threadId: string): Promise<void>;
  /** Opens the delete confirmation; {@link ThreadActions.remove} is what it confirms. */
  requestDelete(threadId: string): void;
  /** Deletes without asking. */
  remove(threadId: string): Promise<void>;
  rename(threadId: string, title: string): Promise<void>;
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

/** The workspace this window works in, live: the selected thread's, else the draft thread's (UI-100). */
export function useCurrentWorkspaceId(): string | undefined {
  const [selected] = useWindowState(SELECTED_THREAD);
  const [draft] = useWindowState(DRAFT_THREAD);
  return workspaceOf(selected, useThreadSummary(selected), draft);
}

/** The workspace this window works in: the selected thread's, else the draft thread's (UI-100). */
export function currentWorkspaceId(host: OmfpHost): string | undefined {
  const selected = host.window.get(SELECTED_THREAD);
  const summary = selected === undefined ? undefined : host.frames.getSnapshot().index.threads.get(selected);
  return workspaceOf(selected, summary, host.window.get(DRAFT_THREAD));
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

function discardDraft(host: OmfpHost): void {
  host.window.set(DRAFT_THREAD, undefined);
  host.window.reset(DRAFT_MESSAGE);
}

/** Deletes `threadIds` first (the daemon refuses a workspace that still has threads), then removes the workspace,
 * which deletes its folder (DMN-028), stopping at the first failure (reported to `ERRORS`); drops the draft thread
 * started in it. Resolves to whether everything worked. */
export async function removeWorkspace(
  host: OmfpHost,
  workspaceId: string,
  threadIds: readonly string[],
): Promise<boolean> {
  if (host.window.get(DRAFT_THREAD)?.workspaceId === workspaceId) {
    discardDraft(host);
  }
  try {
    for (const threadId of threadIds) {
      forget(host, threadId);
      await host.client.call("threads.delete", { threadId });
    }
    await host.client.call("workspaces.remove", { workspaceId });
    return true;
  } catch (error) {
    reportError(host, error);
    return false;
  }
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
    select: (threadId) => {
      const draft = window.get(DRAFT_MESSAGE);
      if (threadId !== undefined && draft.text.trim() === "" && draft.images.length === 0) {
        discardDraft(host);
      }
      window.set(SELECTED_THREAD, threadId);
    },
    openDraft: (workspaceId) => {
      // A new object each time: the draft's composer takes the focus again.
      window.set(DRAFT_THREAD, { workspaceId });
      window.set(SELECTED_THREAD, undefined);
    },
    discardDraft: () => discardDraft(host),
    sendDraft: async () => {
      const draft = window.get(DRAFT_THREAD);
      const prompt = window.get(DRAFT_MESSAGE).text.trim();
      if (draft === undefined || prompt === "") {
        return false;
      }
      try {
        const thread = await client.call("threads.create", { workspace: { id: draft.workspaceId }, prompt });
        if (window.get(DRAFT_THREAD)?.workspaceId === draft.workspaceId) {
          discardDraft(host);
        }
        window.set(SELECTED_THREAD, thread.id);
        return true;
      } catch (error) {
        reportError(host, error);
        return false;
      }
    },
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
    stop: (threadId) => attempt(() => client.call("thread.stop", { threadId })),
    restart: (threadId) => attempt(() => client.call("thread.restart", { threadId })),
  };
}

/** {@link ThreadActions} of this window. */
export function useThreadActions(): ThreadActions {
  const host = useHost();
  return useMemo(() => threadActions(host), [host]);
}
