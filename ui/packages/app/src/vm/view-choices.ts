/**
 * The views a window chose (UI-302, UI-367) as data: per thread the view last shown with it, per workspace the view
 * last chosen in it. Pure functions, unit tested.
 */

/** The view last shown with a thread. */
export interface ThreadViewChoice {
  readonly threadId: string;
  readonly viewId: string;
}

export interface ViewChoices {
  /** Oldest first, at most {@link VIEW_CHOICE_THREADS}. */
  readonly threads: readonly ThreadViewChoice[];
  /** The view last chosen in each workspace, by workspace id. */
  readonly workspaces: Readonly<Record<string, string>>;
}

export const NO_VIEW_CHOICES: ViewChoices = { threads: [], workspaces: {} };

/** How many threads keep their view; the least recently chosen goes first. */
export const VIEW_CHOICE_THREADS = 200;

function workspaceChoice(choices: ViewChoices, workspaceId: string | undefined): string | undefined {
  return workspaceId !== undefined && Object.hasOwn(choices.workspaces, workspaceId)
    ? choices.workspaces[workspaceId]
    : undefined;
}

/**
 * The view to show among `viewIds` (the current workspace's): the thread's, else the workspace's last chosen, else
 * `undefined` (the workspace's default view). A choice the workspace no longer has is skipped, not forgotten.
 */
export function activeView(
  choices: ViewChoices,
  threadId: string | undefined,
  workspaceId: string | undefined,
  viewIds: readonly string[],
): string | undefined {
  const own = threadId === undefined ? undefined : choices.threads.find((choice) => choice.threadId === threadId);
  return [own?.viewId, workspaceChoice(choices, workspaceId)].find(
    (viewId) => viewId !== undefined && viewIds.includes(viewId),
  );
}

/** `threads` with `threadId` on `viewId`, as the most recent choice; the oldest over the bound go. */
function withThread(
  threads: readonly ThreadViewChoice[],
  threadId: string,
  viewId: string,
): readonly ThreadViewChoice[] {
  const last = threads.at(-1);
  if (last?.threadId === threadId && last.viewId === viewId) {
    return threads;
  }
  return [...threads.filter((choice) => choice.threadId !== threadId), { threadId, viewId }].slice(
    -VIEW_CHOICE_THREADS,
  );
}

/** `choices` after switching to `viewId` with `threadId` (none: the draft thread) in `workspaceId`: the thread's view
 * and the workspace's last chosen. */
export function chooseView(
  choices: ViewChoices,
  threadId: string | undefined,
  workspaceId: string | undefined,
  viewId: string,
): ViewChoices {
  const threads = threadId === undefined ? choices.threads : withThread(choices.threads, threadId, viewId);
  const workspaces =
    workspaceId === undefined || workspaceChoice(choices, workspaceId) === viewId
      ? choices.workspaces
      : { ...choices.workspaces, [workspaceId]: viewId };
  return threads === choices.threads && workspaces === choices.workspaces ? choices : { threads, workspaces };
}

/** `choices` once `threadId` shows in `workspaceId`: a thread without a view of its own takes the workspace's last
 * chosen, so later choices in other threads leave it alone. */
export function adoptView(
  choices: ViewChoices,
  threadId: string | undefined,
  workspaceId: string | undefined,
): ViewChoices {
  const last = workspaceChoice(choices, workspaceId);
  if (threadId === undefined || last === undefined || choices.threads.some((choice) => choice.threadId === threadId)) {
    return choices;
  }
  return { ...choices, threads: withThread(choices.threads, threadId, last) };
}

/** `choices` without the threads and workspaces the index no longer lists (deleted); the same object when none. */
export function keepListed(
  choices: ViewChoices,
  threads: { has(id: string): boolean },
  workspaces: { has(id: string): boolean },
): ViewChoices {
  const threadsKept = choices.threads.filter((choice) => threads.has(choice.threadId));
  const entries = Object.entries(choices.workspaces);
  const workspacesKept = entries.filter(([id]) => workspaces.has(id));
  if (threadsKept.length === choices.threads.length && workspacesKept.length === entries.length) {
    return choices;
  }
  return { threads: threadsKept, workspaces: Object.fromEntries(workspacesKept) };
}

/** Reads persisted {@link ViewChoices}, skipping malformed entries; `undefined` when it is not an object. */
export function decodeViewChoices(stored: unknown): ViewChoices | undefined {
  if (typeof stored !== "object" || stored === null || Array.isArray(stored)) {
    return undefined;
  }
  const { threads, workspaces } = stored as { threads?: unknown; workspaces?: unknown };
  const decodedThreads = (Array.isArray(threads) ? threads : []).flatMap((entry: unknown): ThreadViewChoice[] => {
    if (typeof entry !== "object" || entry === null) {
      return [];
    }
    const { threadId, viewId } = entry as { threadId?: unknown; viewId?: unknown };
    return typeof threadId === "string" && typeof viewId === "string" ? [{ threadId, viewId }] : [];
  });
  const decodedWorkspaces =
    typeof workspaces === "object" && workspaces !== null && !Array.isArray(workspaces)
      ? Object.fromEntries(
          Object.entries(workspaces).filter((entry): entry is [string, string] => typeof entry[1] === "string"),
        )
      : {};
  return { threads: decodedThreads.slice(-VIEW_CHOICE_THREADS), workspaces: decodedWorkspaces };
}
