/**
 * The widget SDK's hooks (`omfp/sdk`, docs/specs/08-agent-control.md): every widget, built-in or not,
 * reads daemon state and acts through these, never through the client directly.
 *
 * - Model: {@link useSnapshot} and its shorthands read the per-frame snapshot (UI-012); results are immutable
 *   references, stable while unchanged.
 * - Topics: {@link useThread}, {@link useWorkspace}, {@link useViews} and {@link useTopic} follow their topic while mounted
 *   (reference-counted across widgets), with the protocol's replay/snapshot semantics.
 * - Calls: {@link useCall} is the full protocol (`call("thread.prompt", { threadId, text })`).
 * - Window: {@link useWindowState} for window-local state shared by key; {@link useCommand} registers a command for
 *   the keymap and the `:` line; {@link useRunCommand} runs one; {@link useAnnounce} speaks through the live region.
 */

import type { ConnectionStatus, Index, ThreadView, TopicState } from "@omfp/client";
import {
  type RepoStatus,
  type ThreadSummary,
  type Topic,
  threadTopic,
  type ViewsState,
  viewsTopic,
  type Workspace,
  workspaceTopic,
} from "@omfp/protocol";
import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";
import { type Command, scopesAt } from "./commands.ts";
import type { Snapshot } from "./frame-store.ts";
import { type SdkClient, useHost } from "./host.tsx";
import type { Update, WindowKey } from "./window-store.ts";

/** `select` applied to the latest frame's snapshot; re-renders only when its result changes (by identity). */
export function useSnapshot<T>(select: (snapshot: Snapshot) => T): T {
  const { frames } = useHost();
  return useSyncExternalStore(frames.subscribe, () => select(frames.getSnapshot()));
}

export function useConnection(): ConnectionStatus {
  return useSnapshot((snapshot) => snapshot.connection);
}

/** The `index` topic (the frame follows it for the whole window). */
export function useIndex(): Index {
  return useSnapshot((snapshot) => snapshot.index);
}

/** Thread summaries by id, in creation order. */
export function useThreads(): ReadonlyMap<string, ThreadSummary> {
  return useSnapshot((snapshot) => snapshot.index.threads);
}

export function useThreadSummary(threadId: string | undefined): ThreadSummary | undefined {
  return useSnapshot((snapshot) => (threadId === undefined ? undefined : snapshot.index.threads.get(threadId)));
}

export function useWorkspaces(): ReadonlyMap<string, Workspace> {
  return useSnapshot((snapshot) => snapshot.index.workspaces);
}

/** Follows `topic` while mounted; its subscription state. */
export function useTopic(topic: Topic | undefined): TopicState | undefined {
  const { topics } = useHost();
  useEffect(() => (topic === undefined ? undefined : topics.retain(topic)), [topics, topic]);
  return useSnapshot((snapshot) =>
    topic === undefined ? undefined : snapshot.topics.find((state) => state.topic === topic),
  );
}

/** One thread's reducer state (UI-010), following `thread/<id>` while mounted; `undefined` until its snapshot. */
export function useThread(threadId: string | undefined): ThreadView | undefined {
  useTopic(threadId === undefined ? undefined : threadTopic(threadId));
  return useSnapshot((snapshot) => (threadId === undefined ? undefined : snapshot.threads.get(threadId)));
}

/** A workspace from the index with the git status of its repositories (`workspace/<id>`, followed while mounted). */
export interface WorkspaceData {
  readonly workspace: Workspace | undefined;
  /** `undefined` until the topic's snapshot arrives. */
  readonly status: readonly RepoStatus[] | undefined;
}

export function useWorkspace(workspaceId: string | undefined): WorkspaceData {
  useTopic(workspaceId === undefined ? undefined : workspaceTopic(workspaceId));
  const workspace = useSnapshot((snapshot) =>
    workspaceId === undefined ? undefined : snapshot.index.workspaces.get(workspaceId),
  );
  const status = useSnapshot((snapshot) =>
    workspaceId === undefined ? undefined : snapshot.workspaces.get(workspaceId),
  );
  return { workspace, status };
}

/** A workspace's views manifest, widget folders and widget state (PRT-040), following `views/<id>` while mounted;
 * `undefined` until its snapshot. */
export function useViews(workspaceId: string | undefined): ViewsState | undefined {
  useTopic(workspaceId === undefined ? undefined : viewsTopic(workspaceId));
  return useSnapshot((snapshot) => (workspaceId === undefined ? undefined : snapshot.views.get(workspaceId)));
}

/** The protocol: `call(method, params)` resolves to the typed result or rejects with `CallError`. */
export function useCall(): SdkClient["call"] {
  const { client } = useHost();
  return useCallback<SdkClient["call"]>((method, params) => client.call(method, params), [client]);
}

/** A slot of window-local state and its setter, shared by every widget using the same key. Pass a stable key
 * (module constant, or a memoized factory such as `draftKey`). */
export function useWindowState<T>(key: WindowKey<T>): [T, (update: Update<T>) => void] {
  const { window } = useHost();
  const value = useSyncExternalStore(
    useCallback((listener: () => void) => window.subscribe(key, listener), [window, key]),
    () => window.get(key),
  );
  const set = useCallback((update: Update<T>) => window.set(key, update), [window, key]);
  return [value, set];
}

/**
 * Registers `command` while mounted. The latest `command` object is the one run, so it may close over fresh state;
 * the registration itself changes only with `id`, `scope` or `title`.
 */
export function useCommand(command: Command): void {
  const { commands } = useHost();
  const latest = useRef(command);
  latest.current = command;
  const { id, scope, title } = command;
  useEffect(() => {
    const { aliases } = latest.current;
    return commands.register({
      id,
      title,
      ...(scope === undefined ? {} : { scope }),
      ...(aliases === undefined ? {} : { aliases }),
      args: () => latest.current.args?.() ?? [],
      enabled: () => latest.current.enabled?.() ?? true,
      run: (args) => latest.current.run(args),
    });
  }, [commands, id, scope, title]);
}

/** Runs a command by id or alias in the scopes around the focused element; `false` when nothing answers. */
export function useRunCommand(): (name: string, args?: readonly string[]) => boolean {
  const { commands } = useHost();
  return useCallback(
    (name: string, args: readonly string[] = []) => commands.run(name, args, scopesAt(document.activeElement)),
    [commands],
  );
}

/** Announces `message` through the window's ARIA live region (UI-092). */
export function useAnnounce(): (message: string) => void {
  const { announcer } = useHost();
  return useCallback((message: string) => announcer.announce(message), [announcer]);
}
