/**
 * `omfp:header`, the selected thread's header (UI-030): title with rename in place, the workspace and
 * repository chips, model and thinking selectors, context usage, session cost, the process state with stop/restart,
 * and archive/unarchive/delete (UI-026; delete asks first, in the frame's confirmation dialog). For the draft thread
 * (UI-023): its placeholder title and its workspace's chips. Narrower than {@link COMPACT_BELOW_REM} it is one row
 * (UI-202): title, state and model, the rest in a `⋯` popover.
 */

import type { ThreadSummary } from "@omfp/protocol";
import {
  Button,
  DialogTrigger,
  Icon,
  IconButton,
  type Key,
  Meter,
  PopoverDialog,
  Select,
  StateBadge,
  TextField,
  type ThreadState,
  useWidthClass,
} from "omfp/kit";
import {
  costLabel,
  DRAFT_THREAD,
  type DraftThread,
  decodeModels,
  decodeSession,
  decodeThinkingLevels,
  type HeaderView,
  headerView,
  NO_DETAILS,
  RENAMING_THREAD,
  type SdkClient,
  SELECTED_THREAD,
  splitModel,
  statusLabel,
  strings,
  type ThreadDetails,
  type ThreadStatus,
  useCall,
  useReportError,
  useThread,
  useThreadActions,
  useThreadSummary,
  useWindowState,
  useWorkspaces,
  type WidgetProps,
  WorkspaceChipView,
  workspaceChips,
} from "omfp/sdk";
import { type FormEvent, type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";

const text = strings.shell.header;

/** Below this width (in `rem`) the header is one row (UI-202). */
export const COMPACT_BELOW_REM = 48;

export function HeaderWidget(_props: WidgetProps): ReactNode {
  const [selected] = useWindowState(SELECTED_THREAD);
  const [draft] = useWindowState(DRAFT_THREAD);
  const summary = useThreadSummary(selected);
  if (selected === undefined && draft !== undefined) {
    return <DraftHeader draft={draft} />;
  }
  if (summary === undefined) {
    return (
      <header className="omfp-chat-header omfp-thread-header" data-testid="thread-header" data-empty="true">
        <p className="omfp-thread-header__empty" data-testid="header-empty">
          {text.noThread}
        </p>
      </header>
    );
  }
  return <ThreadHeader key={summary.id} summary={summary} />;
}

function DraftHeader({ draft }: { draft: DraftThread }): ReactNode {
  const workspaces = useWorkspaces();
  const chips = useMemo(() => workspaceChips(draft.workspaceId, [...workspaces.values()]), [draft, workspaces]);
  return (
    <header className="omfp-chat-header omfp-thread-header" data-testid="thread-header" data-draft="true">
      <div className="omfp-thread-header__top">
        <h1
          className="omfp-chat-header__title omfp-thread-header__title"
          data-testid="thread-title"
          data-placeholder="true"
        >
          {strings.shell.untitled}
        </h1>
      </div>
      <div className="omfp-chat-header__bar">
        {chips.map((chip) => (
          <WorkspaceChipView key={chip.key} chip={chip} />
        ))}
      </div>
    </header>
  );
}

/** The header of a thread. It does not read the reducer state, so a streamed delta does not render it again: only
 * {@link SessionCost} follows the thread's events (UI-191). */
function ThreadHeader({ summary }: { summary: ThreadSummary }): ReactNode {
  const workspaces = useWorkspaces();
  const list = useMemo(() => [...workspaces.values()], [workspaces]);
  const [details, refreshSession] = useThreadDetails(summary.id, summary.process.state === "running");
  const view = headerView(summary, details, list);
  const header = useRef<HTMLElement>(null);
  // Its own width decides, wherever the view puts it (UI-202).
  const compact = useWidthClass(header, (width, rem) => width > 0 && width < COMPACT_BELOW_REM * rem);
  return (
    <header
      ref={header}
      className="omfp-chat-header omfp-thread-header"
      data-testid="thread-header"
      data-thread-id={summary.id}
      data-omfp-scope="header"
      data-compact={compact || undefined}
    >
      {compact ? (
        <div className="omfp-thread-header__top">
          <Title threadId={summary.id} view={view} compact />
          <HeaderState status={view.status} />
          <ModelSelect threadId={summary.id} view={view} onChanged={refreshSession} />
          <More threadId={summary.id} view={view} onChanged={refreshSession} />
        </div>
      ) : (
        <>
          <div className="omfp-thread-header__top">
            <Title threadId={summary.id} view={view} />
            <div className="omfp-thread-header__process">
              <HeaderState status={view.status} />
              <ProcessAction threadId={summary.id} view={view} />
            </div>
            <Lifecycle threadId={summary.id} archived={view.archived} />
          </div>
          <div className="omfp-chat-header__bar">
            <Chips view={view} />
            <div className="omfp-thread-header__session">
              <ModelSelect threadId={summary.id} view={view} onChanged={refreshSession} />
              <ThinkingSelect threadId={summary.id} view={view} onChanged={refreshSession} />
              <Usage threadId={summary.id} view={view} />
            </div>
          </div>
        </>
      )}
    </header>
  );
}

/**
 * The compact header's `⋯` (UI-202): a popover with rename, stop or restart, archive and delete, the thinking level,
 * the chips, context usage and cost. An action closes it.
 */
function More({ threadId, view, onChanged }: SessionProps): ReactNode {
  const [, setRenaming] = useWindowState(RENAMING_THREAD);
  return (
    <DialogTrigger>
      <IconButton label={text.more} icon={<Icon name="ellipsis" />} data-testid="header-more" />
      <PopoverDialog title={text.moreTitle} className="omfp-thread-header__more" data-testid="header-more-dialog">
        {({ close }) => (
          <>
            <div className="omfp-thread-header__actions">
              <Button
                size="sm"
                variant="quiet"
                icon={<Icon name="pencil" />}
                data-testid="rename-thread"
                onPress={() => {
                  close();
                  setRenaming(threadId);
                }}
              >
                {text.rename}
              </Button>
              <ProcessAction threadId={threadId} view={view} onDone={close} />
            </div>
            <Lifecycle threadId={threadId} archived={view.archived} onDone={close} />
            <ThinkingSelect threadId={threadId} view={view} onChanged={onChanged} />
            <Chips view={view} />
            <div className="omfp-thread-header__actions">
              <Usage threadId={threadId} view={view} />
            </div>
          </>
        )}
      </PopoverDialog>
    </DialogTrigger>
  );
}

/** `thread.state`, `thread.models` and `thread.thinking_levels` while the process runs; the function refreshes the
 * session after a model or thinking change. */
function useThreadDetails(threadId: string, running: boolean): [ThreadDetails, () => void] {
  const call = useCall();
  const [details, setDetails] = useState<ThreadDetails>(NO_DETAILS);
  useEffect(() => {
    if (!running) {
      return;
    }
    let live = true;
    void fetchDetails(call, threadId).then((fetched) => {
      if (live) {
        setDetails(fetched);
      }
    });
    return () => {
      live = false;
    };
  }, [call, threadId, running]);
  const refresh = () => {
    void call("thread.state", { threadId }).then(
      (session) => setDetails((previous) => ({ ...previous, session: decodeSession(session) })),
      () => {},
    );
  };
  return [details, refresh];
}

async function fetchDetails(call: SdkClient["call"], threadId: string): Promise<ThreadDetails> {
  const [session, models, levels] = await Promise.allSettled([
    call("thread.state", { threadId }),
    call("thread.models", { threadId }),
    call("thread.thinking_levels", { threadId }),
  ]);
  return {
    session: session.status === "fulfilled" ? decodeSession(session.value) : undefined,
    models: models.status === "fulfilled" ? decodeModels(models.value) : [],
    thinkingLevels: levels.status === "fulfilled" ? decodeThinkingLevels(levels.value) : [],
  };
}

/** The title, or its editor while renaming (`Space r`, `:rename`); the compact header renames from its `⋯`. */
function Title({ threadId, view, compact = false }: { threadId: string; view: HeaderView; compact?: boolean }) {
  const [renaming, setRenaming] = useWindowState(RENAMING_THREAD);
  const actions = useThreadActions();
  if (renaming !== threadId) {
    return (
      <>
        <h1
          className={`omfp-chat-header__title omfp-thread-header__title${view.placeholder ? "" : " omfp-grad-text"}`}
          data-testid="thread-title"
          data-placeholder={view.placeholder ? "true" : undefined}
          title={view.title}
        >
          {view.title}
        </h1>
        {!compact && (
          <Button
            size="sm"
            variant="quiet"
            icon={<Icon name="pencil" />}
            data-testid="rename-thread"
            onPress={() => setRenaming(threadId)}
          >
            {text.rename}
          </Button>
        )}
      </>
    );
  }
  return (
    <InlineEditor
      testid="rename-form"
      inputTestId="title-input"
      label={text.renameLabel}
      initial={view.placeholder ? "" : view.title}
      onSave={(title) => {
        setRenaming(undefined);
        if (title !== "") {
          void actions.rename(threadId, title);
        }
      }}
      onCancel={() => setRenaming(undefined)}
    />
  );
}

/** A one-field form: Enter saves, Escape cancels; the field has the focus when it opens. */
function InlineEditor({
  testid,
  inputTestId,
  label,
  initial,
  onSave,
  onCancel,
}: {
  testid: string;
  inputTestId: string;
  label: string;
  initial: string;
  onSave: (value: string) => void;
  onCancel: () => void;
}): ReactNode {
  const [value, setValue] = useState(initial);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSave(value.trim());
  };
  const onEscape = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
    }
  };
  return (
    <form className="omfp-thread-header__editor" data-testid={testid} onSubmit={submit}>
      <TextField
        label={label}
        hideLabel
        value={value}
        onChange={setValue}
        // The editor opens on request and takes the focus (UI-092)
        autoFocus
        inputProps={{ "data-testid": inputTestId, onKeyDown: onEscape }}
      />
      <Button type="submit" size="sm" variant="primary" data-testid={`${testid}-save`}>
        {text.save}
      </Button>
      <Button size="sm" variant="quiet" data-testid={`${testid}-cancel`} onPress={onCancel}>
        {text.cancel}
      </Button>
    </form>
  );
}

function Chips({ view }: { view: HeaderView }): ReactNode {
  return (
    <div className="omfp-thread-header__chips">
      {view.chips.map((chip) => (
        <WorkspaceChipView key={chip.key} chip={chip} />
      ))}
    </div>
  );
}

interface SessionProps {
  readonly threadId: string;
  readonly view: HeaderView;
  /** Refreshes the session after a change. */
  readonly onChanged: () => void;
}

/** Sends a model or thinking change, then refreshes the session; failures go to the frame's error list. */
function useSessionChange(onChanged: () => void): (request: Promise<unknown>) => void {
  const report = useReportError();
  return (request) => {
    request.then(onChanged, report);
  };
}

function ModelSelect({ threadId, view, onChanged }: SessionProps): ReactNode {
  const call = useCall();
  const change = useSessionChange(onChanged);
  const onModel = (key: Key | null) => {
    const model = splitModel(String(key ?? ""));
    if (model !== undefined && String(key) !== view.model) {
      change(call("thread.set_model", { threadId, ...model }));
    }
  };
  return (
    <Select
      label={text.model}
      hideLabel
      icon={<Icon name="brain" />}
      placeholder={text.modelPlaceholder}
      data-testid="model-select"
      className="omfp-thread-header__model"
      items={view.models.map((model) => ({ id: model.value, label: model.label }))}
      selectedKey={view.model ?? null}
      onSelectionChange={onModel}
    />
  );
}

function ThinkingSelect({ threadId, view, onChanged }: SessionProps): ReactNode {
  const call = useCall();
  const change = useSessionChange(onChanged);
  const onThinking = (key: Key | null) => {
    const level = String(key ?? "");
    if (view.thinkingLevels.includes(level) && level !== view.thinking) {
      change(call("thread.set_thinking", { threadId, level }));
    }
  };
  return (
    <Select
      label={text.thinking}
      hideLabel
      placeholder={text.thinkingPlaceholder}
      data-testid="thinking-select"
      items={view.thinkingLevels.map((level) => ({ id: level, label: level }))}
      selectedKey={view.thinking ?? null}
      onSelectionChange={onThinking}
    />
  );
}

/** Context usage and session cost. */
function Usage({ threadId, view }: { threadId: string; view: HeaderView }): ReactNode {
  return (
    <>
      {view.usage !== undefined && (
        <span className="omfp-thread-header__meter" title={view.context}>
          <Meter
            value={view.usage.used}
            max={view.usage.total}
            label={`${text.contextUsage}: ${view.context ?? ""}`}
            data-testid="context-usage"
          />
        </span>
      )}
      <SessionCost threadId={threadId} />
    </>
  );
}

/** The session cost (UI-030); follows the thread while mounted. */
function SessionCost({ threadId }: { threadId: string }): ReactNode {
  const cost = costLabel(useThread(threadId)?.state);
  return (
    cost !== undefined && (
      <span className="omfp-thread-header__cost" data-testid="session-cost" title={text.sessionCost}>
        {cost}
      </span>
    )
  );
}

/** The failure reason and stop/restart; `onDone` runs after pressing it (closes the compact header's popover). */
function ProcessAction({ threadId, view, onDone }: { threadId: string; view: HeaderView; onDone?: () => void }) {
  const actions = useThreadActions();
  return (
    <>
      {view.failure !== undefined && (
        <span className="omfp-thread-header__failure" data-testid="thread-failure">
          {view.failure}
        </span>
      )}
      {view.action !== undefined && (
        <Button
          size="sm"
          variant="quiet"
          icon={<Icon name={view.action === "stop" ? "square" : "refresh-cw"} />}
          data-testid="process-action"
          data-action={view.action}
          onPress={() => {
            onDone?.();
            void (view.action === "stop" ? actions.stop(threadId) : actions.restart(threadId));
          }}
        >
          {view.action === "stop" ? text.stop : text.restart}
        </Button>
      )}
    </>
  );
}

/** Archive (unarchive when archived) and delete, which asks for confirmation first (UI-026). */
function Lifecycle({ threadId, archived, onDone }: { threadId: string; archived: boolean; onDone?: () => void }) {
  const actions = useThreadActions();
  return (
    <div className="omfp-thread-header__lifecycle">
      <Button
        size="sm"
        variant="quiet"
        icon={<Icon name="archive" />}
        data-testid="archive-thread"
        onPress={() => {
          onDone?.();
          void (archived ? actions.unarchive(threadId) : actions.archive(threadId));
        }}
      >
        {archived ? text.unarchive : text.archive}
      </Button>
      <Button
        size="sm"
        variant="danger"
        icon={<Icon name="trash-2" />}
        data-testid="delete-thread"
        onPress={() => {
          onDone?.();
          actions.requestDelete(threadId);
        }}
      >
        {text.delete}
      </Button>
    </div>
  );
}

const BADGE_STATE: Readonly<Record<ThreadStatus, ThreadState>> = {
  awaiting: "awaiting",
  failed: "failed",
  running: "running",
  starting: "running",
  idle: "idle",
  stopped: "idle",
  terminal: "idle",
};

/** The thread's state as a badge (UI-157): glyph and word, popping when it changes after the header opened. */
function HeaderState({ status }: { status: ThreadStatus }): ReactNode {
  const first = useRef(status);
  return (
    <StateBadge
      key={status}
      state={BADGE_STATE[status]}
      fresh={status !== first.current}
      data-testid="thread-state"
      data-status={status}
    >
      {statusLabel(status)}
    </StateBadge>
  );
}
