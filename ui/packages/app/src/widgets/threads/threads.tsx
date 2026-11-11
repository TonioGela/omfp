/**
 * The left column the frame renders outside any view (UI-020..023, UI-026, UI-100): the full-text filter (`/`), the
 * "+ workspace" field, then one section per workspace — name (the current one highlighted), a menu with rename and
 * remove, a delete button, its threads (state badge, title, unread mark), its draft thread, a collapsed "Archived (N)"
 * row and "+ thread". Sections collapse (chevron, `za`); the collapsed set is window-local and survives reloads. Its
 * `threads` scope answers `nav.*` (j/k, gg/G, Ctrl-d/u, UI-095), which are also the global fallback, `dd` and `za` on
 * the focused row or section; it registers `nav.search` (`/`) and `workspace.new` (`Space w`).
 */

import { Button, IconButton, Menu, MenuItem, MenuTrigger, PathField, Popover, SearchField, TextField } from "@omfp/kit";
import { INDEX_TOPIC, type ThreadSummary, type Workspace } from "@omfp/protocol";
import {
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  DELETE_WORKSPACE,
  DRAFT_THREAD,
  errorMessage,
  SELECTED_THREAD,
  threadActions,
  useCall,
  useCommand,
  useHost,
  useReportError,
  useSnapshot,
  useThreadActions,
  useThreads,
  useWindowState,
  useWorkspaces,
  type WidgetProps,
  windowKey,
} from "../../sdk/index.ts";
import { usePathSources } from "../../sdk/paths.ts";
import { strings } from "../../strings/index.ts";
import {
  ConversationSearch,
  decodeIds,
  displayTitle,
  isCollapsed,
  isFiltering,
  isUnread,
  isWorkspacePath,
  type Move,
  moveSelection,
  newWorkspaceParams,
  type Section,
  type Seen,
  sidebarSections,
  type ThreadFilter,
  threadStatus,
  toggled,
  updateSeen,
  visibleOrder,
} from "../../vm/threads.ts";
import { StatusBadge } from "./badges.tsx";

const text = strings.shell.threads;

/** The filter query of this window. */
export const THREAD_QUERY = windowKey<string>("threads/query", "");
/** Workspace ids whose section is collapsed, kept across reloads (UI-022). */
export const COLLAPSED_WORKSPACES = windowKey<readonly string[]>("threads/collapsed", [], decodeIds);
/** Workspace ids whose archived threads are listed (UI-026); collapsed by default. */
export const ARCHIVED_OPEN = windowKey<readonly string[]>("threads/archivedOpen", []);
/** `updatedAt` of each thread when this window last showed it (UI-020). */
export const SEEN = windowKey<Seen>("threads/seen", new Map());

/** Rows moved by Ctrl-d / Ctrl-u. */
const HALF_PAGE = 10;
const NO_SNIPPETS: ReadonlyMap<string, string> = new Map();

export function ThreadsWidget(_props: WidgetProps): ReactNode {
  const threads = useThreadList();
  const workspaces = useWorkspaceList();
  const [selected] = useWindowState(SELECTED_THREAD);
  const [query] = useWindowState(THREAD_QUERY);
  const [collapsed] = useWindowState(COLLAPSED_WORKSPACES);
  const [archivedOpen] = useWindowState(ARCHIVED_OPEN);
  const conversation = useConversationMatches(query);
  const filter = useMemo<ThreadFilter>(() => ({ query, conversation }), [query, conversation]);
  const seen = useSeen(threads, selected);
  const sections = useMemo(() => sidebarSections(threads, workspaces, filter), [threads, workspaces, filter]);
  const order = useMemo(
    () => visibleOrder(sections, collapsed, archivedOpen, filter),
    [sections, collapsed, archivedOpen, filter],
  );
  const root = useRef<HTMLElement>(null);
  const search = useRef<HTMLInputElement>(null);
  const newWorkspace = useRef<HTMLInputElement>(null);
  useNavCommands(order, root);
  useSectionCommands();
  useCommand({
    id: "nav.search",
    title: strings.shell.commands.search,
    aliases: ["search", "filter"],
    run: () => search.current?.focus(),
  });
  useCommand({
    id: "workspace.new",
    title: strings.shell.commands.newWorkspace,
    aliases: ["workspace"],
    run: () => newWorkspace.current?.focus(),
  });
  const loaded = useSnapshot((snapshot) =>
    snapshot.topics.some((topic) => topic.topic === INDEX_TOPIC && topic.sync.kind === "live"),
  );
  const byId = useMemo(() => new Map(threads.map((thread) => [thread.id, thread])), [threads]);
  if (workspaces.length === 0) {
    return (
      <nav ref={root} className="omfp-threads" aria-label={text.label} data-testid="sidebar" data-omfp-scope="threads">
        <NewWorkspace inputRef={newWorkspace} focus={loaded} />
        {loaded && (
          <p className="omfp-muted" data-testid="no-workspaces-hint">
            {strings.shell.newWorkspace.hint}
          </p>
        )}
      </nav>
    );
  }
  return (
    <nav ref={root} className="omfp-threads" aria-label={text.label} data-testid="sidebar" data-omfp-scope="threads">
      <ThreadFilterField search={search} />
      <NewWorkspace inputRef={newWorkspace} focus={false} />
      {sections.map((section) => (
        <WorkspaceSection
          key={section.workspace.id}
          section={section}
          filter={filter}
          byId={byId}
          seen={seen}
          selected={selected}
        />
      ))}
      {sections.length === 0 && (
        <p className="omfp-muted" data-testid="no-threads">
          {text.noMatch}
        </p>
      )}
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

/** Conversation matches of `query` by `threads.search` (UI-022, DMN-076): debounced, only for the current query. */
function useConversationMatches(query: string): ReadonlyMap<string, string> {
  const call = useCall();
  const [found, setFound] = useState<{ query: string; snippets: ReadonlyMap<string, string> }>({
    query: "",
    snippets: NO_SNIPPETS,
  });
  const search = useMemo(
    () =>
      new ConversationSearch(
        async (wanted) => (await call("threads.search", { query: wanted })).results,
        (run, ms) => {
          const timer = setTimeout(run, ms);
          return () => clearTimeout(timer);
        },
        (answered, snippets) => setFound({ query: answered, snippets }),
      ),
    [call],
  );
  useEffect(() => () => search.dispose(), [search]);
  useEffect(() => search.update(query), [search, query]);
  return found.query === query ? found.snippets : NO_SNIPPETS;
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

/** `dd` (delete the focused thread, draft or workspace) and `za` (collapse the focused section), UI-020, UI-022. */
function useSectionCommands(): void {
  const host = useHost();
  const actions = useThreadActions();
  const sectionOf = (element: Element | null) =>
    element?.closest<HTMLElement>('[data-testid="workspace-section"]')?.dataset.workspaceId;
  useCommand({
    id: "nav.delete",
    title: strings.shell.commands.deleteFocused,
    scope: "threads",
    run: () => {
      const element = document.activeElement;
      const threadId = element?.closest<HTMLElement>("[data-thread-id]")?.dataset.threadId;
      const workspaceId = sectionOf(element);
      if (threadId !== undefined) {
        actions.requestDelete(threadId);
      } else if (element?.closest('[data-testid="draft-thread"]')) {
        actions.discardDraft();
      } else if (workspaceId !== undefined) {
        host.window.set(DELETE_WORKSPACE, workspaceId);
      }
    },
  });
  useCommand({
    id: "nav.toggleSection",
    title: strings.shell.commands.toggleSection,
    scope: "threads",
    run: () => {
      const workspaceId = sectionOf(document.activeElement);
      if (workspaceId !== undefined) {
        host.window.set(COLLAPSED_WORKSPACES, (previous) => toggled(previous, workspaceId));
      }
    },
  });
}

/** The one filter of the column (UI-022); `/` focuses it. */
function ThreadFilterField({ search }: { search: RefObject<HTMLInputElement | null> }): ReactNode {
  const [query, setQuery] = useWindowState(THREAD_QUERY);
  return (
    <div className="omfp-threads__filter">
      <SearchField
        label={text.filter}
        hideLabel
        placeholder={text.filterPlaceholder}
        value={query}
        onChange={setQuery}
        inputRef={search}
        inputProps={{ "data-testid": "thread-filter" }}
        clearTestId="clear-thread-filter"
      />
    </div>
  );
}

/** "+ workspace" (UI-023): one field taking a name (optional) or, when it starts with `/` or `~`, the path of a
 * repository to link (completed as in DMN-021); Enter creates the workspace and opens a draft thread in it. */
function NewWorkspace({
  inputRef,
  focus,
}: {
  inputRef: RefObject<HTMLInputElement | null>;
  focus: boolean;
}): ReactNode {
  const call = useCall();
  const actions = useThreadActions();
  const sources = usePathSources();
  const labels = strings.shell.newWorkspace;
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  useEffect(() => {
    if (focus) {
      inputRef.current?.focus();
    }
  }, [focus, inputRef]);
  const submit = (typed: string) => {
    if (busy) {
      return;
    }
    setBusy(true);
    setError(undefined);
    call("workspaces.create", newWorkspaceParams(typed)).then(
      (workspace) => {
        setBusy(false);
        setValue("");
        actions.openDraft(workspace.id);
      },
      (failure: unknown) => {
        setBusy(false);
        setError(errorMessage(failure));
      },
    );
  };
  return (
    <div
      className="omfp-threads__new-workspace"
      data-testid="new-workspace"
      data-mode={isWorkspacePath(value) ? "path" : "name"}
    >
      <PathField
        label={labels.label}
        hideLabel
        placeholder={labels.placeholder}
        value={value}
        onChange={setValue}
        onSubmit={submit}
        complete={sources.complete}
        inputRef={inputRef}
        data-testid="new-workspace-input"
      />
      {error !== undefined && (
        <p className="omfp-form-error" role="alert" data-testid="new-workspace-error">
          {error}
        </p>
      )}
    </div>
  );
}

function WorkspaceSection({
  section,
  filter,
  byId,
  seen,
  selected,
}: {
  section: Section;
  filter: ThreadFilter;
  byId: ReadonlyMap<string, ThreadSummary>;
  seen: Seen;
  selected: string | undefined;
}): ReactNode {
  const host = useHost();
  const actions = useThreadActions();
  const { workspace } = section;
  const [collapsedIds, setCollapsed] = useWindowState(COLLAPSED_WORKSPACES);
  const [archivedOpen, setArchivedOpen] = useWindowState(ARCHIVED_OPEN);
  const [draft] = useWindowState(DRAFT_THREAD);
  const [renaming, setRenaming] = useState(false);
  const headingId = useId();
  const collapsed = isCollapsed(workspace.id, collapsedIds, filter);
  const current = useCurrentWorkspace() === workspace.id;
  const showArchived = archivedOpen.includes(workspace.id);
  const draftHere = draft?.workspaceId === workspace.id && !isFiltering(filter);
  const snippets = isFiltering(filter) ? filter.conversation : NO_SNIPPETS;
  return (
    <section
      className="omfp-ws"
      data-testid="workspace-section"
      data-workspace-id={workspace.id}
      data-current={current ? "true" : undefined}
      aria-labelledby={headingId}
    >
      <div className="omfp-ws__header">
        <button
          type="button"
          className="omfp-ws__chevron"
          data-testid="workspace-collapse"
          aria-expanded={!collapsed}
          aria-label={collapsed ? text.expand(workspace.name) : text.collapse(workspace.name)}
          onClick={() => setCollapsed((previous) => toggled(previous, workspace.id))}
        >
          <span aria-hidden="true">{collapsed ? "▸" : "▾"}</span>
        </button>
        {renaming ? (
          <RenameWorkspace workspace={workspace} close={() => setRenaming(false)} />
        ) : (
          <h3 id={headingId} className="omfp-ws__name" data-testid="workspace-section-name">
            {workspace.name}
            {current && <span className="omfp-visually-hidden"> ({text.current})</span>}
          </h3>
        )}
        <MenuTrigger>
          <IconButton label={text.menu(workspace.name)} icon="⋯" data-testid="workspace-menu" />
          <Popover placement="bottom end">
            <Menu
              aria-label={text.menu(workspace.name)}
              onAction={(key) => {
                if (key === "rename") {
                  setRenaming(true);
                } else if (key === "remove") {
                  host.window.set(DELETE_WORKSPACE, workspace.id);
                }
              }}
            >
              <MenuItem id="rename" data-testid="workspace-rename">
                {text.rename}
              </MenuItem>
              <MenuItem id="remove" data-testid="workspace-remove">
                {text.remove}
              </MenuItem>
            </Menu>
          </Popover>
        </MenuTrigger>
        <IconButton
          className="omfp-reveal"
          label={text.deleteWorkspace(workspace.name)}
          icon="✕"
          data-testid="delete-workspace"
          onPress={() => host.window.set(DELETE_WORKSPACE, workspace.id)}
        />
      </div>
      {!collapsed && (
        <>
          <ul className="omfp-threads__list">
            {section.threadIds.map((id) => {
              const thread = byId.get(id);
              return thread === undefined ? null : (
                <ThreadRow
                  key={id}
                  thread={thread}
                  selected={selected === id}
                  unread={isUnread(thread, seen, selected)}
                  snippet={snippets.get(id)}
                />
              );
            })}
            {draftHere && (
              <li className="omfp-thread-item">
                <button
                  type="button"
                  className="omfp-thread-row"
                  data-testid="draft-thread"
                  data-workspace-id={workspace.id}
                  aria-current={selected === undefined ? "true" : undefined}
                  onClick={() => actions.openDraft(workspace.id)}
                >
                  <span className="omfp-thread-row__title" data-placeholder="true">
                    {strings.shell.untitled}
                  </span>
                  <span className="omfp-muted">{text.draft}</span>
                </button>
              </li>
            )}
          </ul>
          {section.archivedIds.length > 0 && (
            <div className="omfp-ws__archived">
              <button
                type="button"
                className="omfp-ws__archived-toggle"
                data-testid="archived-toggle"
                aria-expanded={showArchived}
                onClick={() => setArchivedOpen((previous) => toggled(previous, workspace.id))}
              >
                {text.archived(section.archivedIds.length)}
              </button>
              {showArchived && (
                <ul className="omfp-threads__list" data-testid="archived-list">
                  {section.archivedIds.map((id) => {
                    const thread = byId.get(id);
                    return thread === undefined ? null : (
                      <ThreadRow
                        key={id}
                        thread={thread}
                        selected={selected === id}
                        unread={false}
                        snippet={snippets.get(id)}
                        archived
                      />
                    );
                  })}
                </ul>
              )}
            </div>
          )}
          <Button
            size="sm"
            variant="quiet"
            className="omfp-ws__add"
            data-testid="add-thread"
            data-workspace-id={workspace.id}
            onPress={() => actions.openDraft(workspace.id)}
          >
            {text.addThread}
          </Button>
        </>
      )}
    </section>
  );
}

/** The workspace this window works in: the selected thread's, else the draft thread's. */
function useCurrentWorkspace(): string | undefined {
  const [selected] = useWindowState(SELECTED_THREAD);
  const [draft] = useWindowState(DRAFT_THREAD);
  const threads = useThreads();
  return selected === undefined ? draft?.workspaceId : threads.get(selected)?.workspaceId;
}

function RenameWorkspace({ workspace, close }: { workspace: Workspace; close: () => void }): ReactNode {
  const call = useCall();
  const report = useReportError();
  const [name, setName] = useState(workspace.name);
  const save = (event: FormEvent) => {
    event.preventDefault();
    close();
    const trimmed = name.trim();
    if (trimmed !== "" && trimmed !== workspace.name) {
      call("workspaces.update", { workspaceId: workspace.id, name: trimmed }).catch(report);
    }
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  };
  return (
    <form className="omfp-ws__rename" onSubmit={save}>
      <TextField
        label={text.renameLabel}
        hideLabel
        value={name}
        onChange={setName}
        onKeyDown={onKeyDown}
        // Opened from the section's menu to type the new name at once
        autoFocus
        inputProps={{ "data-testid": "workspace-name-input" }}
      />
    </form>
  );
}

function ThreadRow({
  thread,
  selected,
  unread,
  snippet,
  archived = false,
}: {
  thread: ThreadSummary;
  selected: boolean;
  unread: boolean;
  snippet: string | undefined;
  archived?: boolean;
}): ReactNode {
  const actions = useThreadActions();
  const status = threadStatus(thread);
  const title = displayTitle(thread);
  return (
    <li className="omfp-thread-item">
      <button
        type="button"
        className="omfp-thread-row"
        data-testid="thread-row"
        data-thread-id={thread.id}
        data-status={status}
        data-title-source={thread.titleSource}
        data-archived={archived ? "true" : undefined}
        aria-current={selected ? "true" : undefined}
        onClick={() => actions.select(thread.id)}
      >
        <StatusBadge status={status} testid="thread-status" />
        <span className="omfp-thread-row__title">{title}</span>
        {unread && (
          <span className="omfp-thread-row__unread" data-testid="unread" role="img" aria-label={text.unread}>
            •
          </span>
        )}
        {snippet !== undefined && (
          <span className="omfp-thread-row__snippet" data-testid="thread-snippet">
            {snippet}
          </span>
        )}
      </button>
      {archived && (
        <Button
          size="sm"
          variant="quiet"
          aria-label={text.unarchiveThread(title)}
          data-testid="unarchive-thread"
          data-thread-id={thread.id}
          onPress={() => void actions.unarchive(thread.id)}
        >
          {text.unarchive}
        </Button>
      )}
      <IconButton
        className="omfp-reveal"
        label={text.deleteThread(title)}
        icon="✕"
        data-testid="row-delete-thread"
        data-thread-id={thread.id}
        onPress={() => actions.requestDelete(thread.id)}
      />
    </li>
  );
}
