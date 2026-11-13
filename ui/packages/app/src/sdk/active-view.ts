/**
 * The view a window shows per thread (UI-302, UI-367): read from and written to {@link VIEW_CHOICES}. Every way of
 * switching view (mode-line tabs, `:view`, `] v` / `[ v`, init files' `omfp.run("view.switch")`) goes through
 * {@link chooseView}.
 */

import { INDEX_TOPIC } from "@omfp/protocol";
import { useEffect } from "react";
import { activeView, adoptView, chooseView as choose, keepListed } from "../vm/view-choices.ts";
import { currentWorkspaceId, useCurrentWorkspaceId } from "./actions.ts";
import { useWindowState } from "./hooks.ts";
import { type OmfpHost, useHost } from "./host.tsx";
import { SELECTED_THREAD, VIEW_CHOICES } from "./keys.ts";

/** The id among `viewIds` (the current workspace's views) this window shows for the selected thread (else the draft
 * thread): the thread's view, else the one last chosen in its workspace, else `undefined` (the default view). */
export function useActiveView(viewIds: readonly string[]): string | undefined {
  const [choices] = useWindowState(VIEW_CHOICES);
  const [selected] = useWindowState(SELECTED_THREAD);
  return activeView(choices, selected, useCurrentWorkspaceId(), viewIds);
}

/** Switches this window to `viewId`: the selected thread's view (none: the draft thread's) and its workspace's last
 * chosen. */
export function chooseView(host: OmfpHost, viewId: string): void {
  const selected = host.window.get(SELECTED_THREAD);
  host.window.set(VIEW_CHOICES, (choices) => choose(choices, selected, currentWorkspaceId(host), viewId));
}

/**
 * Keeps {@link VIEW_CHOICES} up to date, for the frame: a thread shown for the first time takes the view last chosen
 * in its workspace, and the threads and workspaces the live index no longer lists (deleted) are dropped.
 */
export function useViewChoicesUpkeep(): void {
  const host = useHost();
  const [selected] = useWindowState(SELECTED_THREAD);
  const workspaceId = useCurrentWorkspaceId();
  useEffect(() => {
    host.window.set(VIEW_CHOICES, (choices) => adoptView(choices, selected, workspaceId));
  }, [host, selected, workspaceId]);
  useEffect(() => {
    const prune = () => {
      const snapshot = host.frames.getSnapshot();
      if (snapshot.topics.find((state) => state.topic === INDEX_TOPIC)?.sync.kind !== "live") {
        return;
      }
      const { threads, workspaces } = snapshot.index;
      host.window.set(VIEW_CHOICES, (choices) => keepListed(choices, threads, workspaces));
    };
    prune();
    return host.frames.subscribe(() => {
      if (host.frames.getSnapshot().dirty.index) {
        prune();
      }
    });
  }, [host]);
}
