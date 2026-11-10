/**
 * `omfp:threads`, the thread list (UI-020..022, UI-026): state badges, unread marks and workspace chips; grouping, a
 * key filter (or the Archived threads only) and free-text search; `+ thread` and "next waiting"; the colour scheme
 * and density pickers (UI-091, UI-094). Its `threads` scope answers `nav.*` (j/k, gg/G, Ctrl-d/u, UI-095), which are
 * also the global fallback, and it registers `nav.search` (`/`).
 */

import { Button, type Key, SearchField, Select } from "@omfp/kit";
import type { Density, ThreadSummary, Workspace } from "@omfp/protocol";
import { type ReactNode, type RefObject, useEffect, useMemo, useRef } from "react";
import {
  COLOR_SCHEME,
  type ColorScheme,
  DEFAULT_DENSITY,
  NEW_THREAD_DIALOG,
  SELECTED_THREAD,
  threadActions,
  useCommand,
  useHost,
  useSetSettings,
  useSettings,
  useThreadActions,
  useThreads,
  useWindowState,
  useWorkspaces,
  type WidgetProps,
  windowKey,
} from "../../sdk/index.ts";
import { strings } from "../../strings/index.ts";
import {
  DEFAULT_GROUPING,
  DIMENSIONS,
  type Dimension,
  displayTitle,
  type Filter,
  filterOptions,
  type Grouping,
  groupThreads,
  isGrouping,
  isUnread,
  listOrder,
  type Move,
  moveSelection,
  NO_FILTER,
  nextNeedingAttention,
  type Seen,
  threadStatus,
  updateSeen,
  waitingCount,
  workspaceChips,
} from "../../vm/threads.ts";
import { StatusBadge, WorkspaceChipView } from "./badges.tsx";

const text = strings.shell.threads;

/** The list's grouping, kept across reloads. */
export const THREAD_GROUPING = windowKey<Grouping>("threads/grouping", DEFAULT_GROUPING, (stored) =>
  isGrouping(stored) ? stored : undefined,
);
export const THREAD_FILTER = windowKey<Filter>("threads/filter", NO_FILTER);
/** `updatedAt` of each thread when this window last showed it (UI-020). */
export const SEEN = windowKey<Seen>("threads/seen", new Map());

/** Rows moved by Ctrl-d / Ctrl-u. */
const HALF_PAGE = 10;
const ALL = "all";
const ARCHIVED = "archived";

export function ThreadsWidget(_props: WidgetProps): ReactNode {
  const threads = useThreadList();
  const workspaces = useWorkspaceList();
  const [selected] = useWindowState(SELECTED_THREAD);
  const [grouping] = useWindowState(THREAD_GROUPING);
  const [filter] = useWindowState(THREAD_FILTER);
  const seen = useSeen(threads, selected);
  const groups = useMemo(
    () => groupThreads(threads, workspaces, grouping, filter),
    [threads, workspaces, grouping, filter],
  );
  const order = useMemo(() => listOrder(groups), [groups]);
  const root = useRef<HTMLElement>(null);
  const search = useRef<HTMLInputElement>(null);
  useNavCommands(order, root);
  useCommand({
    id: "nav.search",
    title: strings.shell.commands.search,
    aliases: ["search"],
    run: () => search.current?.focus(),
  });
  const byId = useMemo(() => new Map(threads.map((thread) => [thread.id, thread])), [threads]);
  return (
    <nav ref={root} className="omfp-threads" aria-label={text.label} data-testid="sidebar" data-omfp-scope="threads">
      <Actions threads={threads} />
      <Controls threads={threads} workspaces={workspaces} search={search} />
      {groups.map((group) => (
        <section key={group.key} className="omfp-threads__group" data-testid="thread-group" data-group={group.key}>
          {grouping !== "none" && <h3 className="omfp-threads__label">{group.label}</h3>}
          <ul className="omfp-threads__list">
            {group.threadIds.map((id) => {
              const thread = byId.get(id);
              return thread === undefined ? null : (
                <ThreadRow
                  key={id}
                  thread={thread}
                  workspaces={workspaces}
                  selected={selected === id}
                  unread={isUnread(thread, seen, selected)}
                />
              );
            })}
          </ul>
        </section>
      ))}
      {order.length === 0 && (
        <p className="omfp-muted" data-testid="no-threads">
          {text.empty}
        </p>
      )}
      <Appearance />
    </nav>
  );
}

function useThreadList(): readonly ThreadSummary[] {
  const threads = useThreads();
  return useMemo(() => [...threads.values()], [threads]);
}

function useWorkspaceList(): readonly Workspace[] {
  const workspaces = useWorkspaces();
  return useMemo(() => [...workspaces.values()], [workspaces]);
}

/** Unread baselines, kept in step with the index; showing a thread marks it seen (UI-020). */
function useSeen(threads: readonly ThreadSummary[], selected: string | undefined): Seen {
  const [seen, setSeen] = useWindowState(SEEN);
  useEffect(() => {
    setSeen((previous) => updateSeen(previous, threads, selected));
  }, [threads, selected, setSeen]);
  return seen;
}

/** `nav.*` in the `threads` scope and as the global fallback. */
function useNavCommands(order: readonly string[], root: RefObject<HTMLElement | null>): void {
  const host = useHost();
  const latest = useRef(order);
  latest.current = order;
  useEffect(() => {
    const actions = threadActions(host);
    const move = (step: Move) => {
      const target = moveSelection(latest.current, host.window.get(SELECTED_THREAD), step);
      if (target === undefined) {
        return;
      }
      actions.select(target);
      const row = root.current?.querySelector<HTMLElement>(
        `[data-testid="thread-row"][data-thread-id="${CSS.escape(target)}"]`,
      );
      if (root.current?.contains(document.activeElement) === true) {
        row?.focus();
      }
      row?.scrollIntoView?.({ block: "nearest" });
    };
    const removals = NAV_MOVES.flatMap(([id, title, step]) => [
      host.commands.register({ id, title, scope: "threads", run: () => move(step) }),
      host.commands.register({ id, title, run: () => move(step) }),
    ]);
    return () => {
      for (const remove of removals) {
        remove();
      }
    };
  }, [host, root]);
}

const NAV_MOVES: ReadonlyArray<readonly [string, string, Move]> = [
  ["nav.next", strings.shell.commands.next, 1],
  ["nav.prev", strings.shell.commands.prev, -1],
  ["nav.top", strings.shell.commands.top, "top"],
  ["nav.bottom", strings.shell.commands.bottom, "bottom"],
  ["nav.halfDown", strings.shell.commands.halfDown, HALF_PAGE],
  ["nav.halfUp", strings.shell.commands.halfUp, -HALF_PAGE],
];

/** New thread and the "next thread needing attention" shortcut (UI-021). */
function Actions({ threads }: { threads: readonly ThreadSummary[] }): ReactNode {
  const [, openNewThread] = useWindowState(NEW_THREAD_DIALOG);
  const [selected] = useWindowState(SELECTED_THREAD);
  const actions = useThreadActions();
  const waiting = waitingCount(threads);
  return (
    <div className="omfp-threads__actions">
      <Button variant="primary" size="sm" data-testid="new-thread" onPress={() => openNewThread(true)}>
        {text.newThread}
      </Button>
      <Button
        size="sm"
        data-testid="next-attention"
        isDisabled={waiting === 0}
        onPress={() => {
          const next = nextNeedingAttention(threads, selected);
          if (next !== undefined) {
            actions.select(next);
          }
        }}
      >
        {text.nextWaiting(waiting)}
      </Button>
    </div>
  );
}

/** Grouping, key filter (or Archived), free-text search (UI-022, UI-026). */
function Controls({
  threads,
  workspaces,
  search,
}: {
  threads: readonly ThreadSummary[];
  workspaces: readonly Workspace[];
  search: RefObject<HTMLInputElement | null>;
}): ReactNode {
  const [grouping, setGrouping] = useWindowState(THREAD_GROUPING);
  const [filter, setFilter] = useWindowState(THREAD_FILTER);
  const options = useMemo(() => filterOptions(threads, workspaces), [threads, workspaces]);
  const dimension = strings.shell.dimension;
  const groupings = [
    { id: "none", label: text.groupingNone },
    ...DIMENSIONS.map((id) => ({ id, label: dimension[id] })),
    { id: "pr", label: dimension.pr },
  ];
  const filters = [
    { id: ALL, label: text.filterAll },
    { id: ARCHIVED, label: text.filterArchived },
    ...options.map((option) => ({
      id: filterId(option.dimension, option.key.key),
      label: text.filterOption(dimension[option.dimension], option.key.label),
    })),
  ];
  return (
    <div className="omfp-threads__controls">
      <Select
        label={text.grouping}
        data-testid="grouping"
        items={groupings}
        disabledKeys={["pr"]}
        selectedKey={grouping}
        onSelectionChange={(key) => {
          if (isGrouping(key)) {
            setGrouping(key);
          }
        }}
      />
      <Select
        label={text.filter}
        data-testid="filter-key"
        items={filters}
        selectedKey={selectedFilter(filter)}
        onSelectionChange={(key) => setFilter((previous) => applyFilterKey(previous, key))}
      />
      <SearchField
        label={text.search}
        hideLabel
        placeholder={text.search}
        value={filter.text}
        onChange={(value) => setFilter((previous) => ({ ...previous, text: value }))}
        inputRef={search}
        inputProps={{ "data-testid": "filter-text" }}
        clearTestId="clear-filter-text"
      />
    </div>
  );
}

function filterId(dimension: Dimension, key: string): string {
  return `${dimension}:${key}`;
}

function selectedFilter(filter: Filter): string {
  if (filter.archived) {
    return ARCHIVED;
  }
  return filter.key === undefined ? ALL : filterId(filter.key.dimension, filter.key.key);
}

function applyFilterKey(filter: Filter, key: Key | null): Filter {
  const value = String(key ?? ALL);
  const { key: _dropped, ...rest } = filter;
  if (value === ALL || value === ARCHIVED) {
    return { ...rest, archived: value === ARCHIVED };
  }
  const colon = value.indexOf(":");
  const dimension = DIMENSIONS.find((candidate) => candidate === value.slice(0, colon));
  return dimension === undefined
    ? { ...rest, archived: false }
    : { ...rest, archived: false, key: { dimension, key: value.slice(colon + 1) } };
}

function ThreadRow({
  thread,
  workspaces,
  selected,
  unread,
}: {
  thread: ThreadSummary;
  workspaces: readonly Workspace[];
  selected: boolean;
  unread: boolean;
}): ReactNode {
  const actions = useThreadActions();
  const status = threadStatus(thread);
  const chips = workspaceChips(thread.workspaceId, workspaces);
  return (
    <li>
      <button
        type="button"
        className="omfp-thread-row"
        data-testid="thread-row"
        data-thread-id={thread.id}
        data-status={status}
        data-title-source={thread.titleSource}
        aria-current={selected ? "true" : undefined}
        onClick={() => actions.select(thread.id)}
      >
        <StatusBadge status={status} testid="thread-status" />
        <span className="omfp-thread-row__title">{displayTitle(thread)}</span>
        {unread && (
          <span className="omfp-thread-row__unread" data-testid="unread" role="img" aria-label={text.unread}>
            •
          </span>
        )}
        {chips.length > 0 && (
          <span className="omfp-thread-row__chips">
            {chips.map((chip) => (
              <WorkspaceChipView key={chip.key} chip={chip} />
            ))}
          </span>
        )}
      </button>
    </li>
  );
}

/** Colour scheme (UI-091, this window, persisted) and density (the daemon's setting, every window, UI-097). */
function Appearance(): ReactNode {
  const [scheme, setScheme] = useWindowState(COLOR_SCHEME);
  const density = useSettings().ui.density ?? DEFAULT_DENSITY;
  const setSettings = useSetSettings();
  const schemes = strings.shell.colorScheme;
  const densities = strings.shell.densities;
  return (
    <div className="omfp-threads__footer">
      <Select
        label={text.theme}
        data-testid="theme-mode"
        items={(["system", "light", "dark"] as const).map((id) => ({ id, label: schemes[id] }))}
        selectedKey={scheme}
        onSelectionChange={(key) => {
          if (key === "system" || key === "light" || key === "dark") {
            setScheme(key satisfies ColorScheme);
          }
        }}
      />
      <Select
        label={text.density}
        data-testid="density-mode"
        items={(["compact", "normal"] as const).map((id) => ({ id, label: densities[id] }))}
        selectedKey={density}
        onSelectionChange={(key) => {
          if ((key === "compact" || key === "normal") && key !== density) {
            void setSettings({ ui: { density: key satisfies Density } });
          }
        }}
      />
    </div>
  );
}
