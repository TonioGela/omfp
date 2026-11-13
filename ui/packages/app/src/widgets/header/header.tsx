/**
 * `omfp:header`, the selected thread's header (UI-030): title with rename in place, the workspace chip (UI-340),
 * model and thinking selectors, context usage, session cost, the process state with stop/restart, the restart the
 * daemon holds back named by its reasons (UI-349), and archive/unarchive/delete (UI-026; delete asks first, in the
 * frame's confirmation dialog). For the draft thread (UI-023): its placeholder title and its workspace's chip. The
 * title row gives the title the room (UI-482): the state, one state-dependent action and a `⋯` popover holding rename,
 * archive and delete. Narrower than {@link COMPACT_BELOW_REM} it is one row (UI-202): title, state and model, the rest
 * in the `⋯` popover. When omp has no model it can use — its start failed saying so, or it lists no model — it offers
 * Log in (UI-388).
 */

import type { ThreadSummary } from "@omfp/protocol";
import {
  Button,
  ChatHeader,
  ChatTitle,
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
  useMediaQuery,
  useWidthClass,
} from "omfp/kit";
import {
  AUTH_REVISION,
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
  REVEAL_WIDGET,
  requestLogin,
  type SdkClient,
  SELECTED_THREAD,
  splitModel,
  statusLabel,
  strings,
  type ThreadDetails,
  type ThreadStatus,
  useCall,
  useHost,
  useReportError,
  useRunningInBackground,
  useThread,
  useThreadActions,
  useThreadSummary,
  useWindowState,
  useWorkspaces,
  type WidgetProps,
  type WorkspaceChip,
  workspaceChip,
} from "omfp/sdk";
import { type FormEvent, type KeyboardEvent, type ReactNode, useEffect, useId, useMemo, useRef, useState } from "react";

const text = strings.shell.header;

/** Below this width (in `rem`) the header is one row (UI-202). */
const COMPACT_BELOW_REM = 48;
/** A touch screen too short for two header rows: a phone held sideways (UI-419). */
const SHORT_TOUCH_SCREEN = "(pointer: coarse) and (max-height: 500px)";

export function HeaderWidget(_props: WidgetProps): ReactNode {
  const [selected] = useWindowState(SELECTED_THREAD);
  const [draft] = useWindowState(DRAFT_THREAD);
  const summary = useThreadSummary(selected);
  if (selected === undefined && draft !== undefined) {
    return <DraftHeader draft={draft} />;
  }
  if (summary === undefined) {
    return (
      <ChatHeader className="omfp-thread-header" data-testid="thread-header" data-empty="true">
        <p className="omfp-thread-header__empty" data-testid="header-empty">
          {text.noThread}
        </p>
      </ChatHeader>
    );
  }
  return <ThreadHeader key={summary.id} summary={summary} />;
}

function DraftHeader({ draft }: { draft: DraftThread }): ReactNode {
  const workspaces = useWorkspaces();
  const chip = useMemo(() => workspaceChip(draft.workspaceId, [...workspaces.values()]), [draft, workspaces]);
  return (
    <ChatHeader
      className="omfp-thread-header"
      data-testid="thread-header"
      data-draft="true"
      bar={<WorkspaceChipButton chip={chip} />}
    >
      <ChatTitle data-testid="thread-title" placeholder>
        {strings.shell.untitled}
      </ChatTitle>
    </ChatHeader>
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
  // Its own width decides, wherever the view puts it (UI-202); a phone held sideways is always compact (UI-419).
  const narrow = useWidthClass(header, (width, rem) => width > 0 && width < COMPACT_BELOW_REM * rem);
  const compact = useMediaQuery(SHORT_TOUCH_SCREEN) || narrow;
  return (
    <ChatHeader
      ref={header}
      className="omfp-thread-header"
      data-testid="thread-header"
      data-thread-id={summary.id}
      data-omfp-scope="header"
      data-compact={compact || undefined}
      bar={
        compact ? undefined : (
          <>
            <Chips view={view} />
            <div className="omfp-thread-header__session">
              <ModelSelect threadId={summary.id} view={view} onChanged={refreshSession} />
              <ThinkingSelect threadId={summary.id} view={view} onChanged={refreshSession} />
              <Usage threadId={summary.id} view={view} />
            </div>
          </>
        )
      }
    >
      {compact ? (
        <>
          <Title threadId={summary.id} view={view} />
          <HeaderState status={view.status} failure={view.failure} />
          <BackgroundBadge threadId={summary.id} />
          <PendingRestart threadId={summary.id} view={view} />
          <ModelSelect threadId={summary.id} view={view} onChanged={refreshSession} />
          <More threadId={summary.id} view={view} onChanged={refreshSession} compact />
        </>
      ) : (
        <>
          <Title threadId={summary.id} view={view} />
          <div className="omfp-thread-header__process">
            <HeaderState status={view.status} failure={view.failure} />
            <BackgroundBadge threadId={summary.id} />
            <ProcessAction threadId={summary.id} view={view} />
            <PendingRestart threadId={summary.id} view={view} />
            <More threadId={summary.id} view={view} onChanged={refreshSession} compact={false} />
          </div>
        </>
      )}
    </ChatHeader>
  );
}

/**
 * The header's `⋯` (UI-482): a popover with why the process failed, rename, archive and delete; the compact header's
 * (UI-202) adds stop or restart, the thinking level, the chips, context usage and cost. An action closes it.
 */
function More({ threadId, view, onChanged, compact }: SessionProps & { compact: boolean }): ReactNode {
  const [, setRenaming] = useWindowState(RENAMING_THREAD);
  return (
    <DialogTrigger>
      <IconButton variant="secondary" label={text.more} icon={<Icon name="ellipsis" />} data-testid="header-more" />
      <PopoverDialog title={text.moreTitle} className="omfp-thread-header__more" data-testid="header-more-dialog">
        {({ close }) => (
          <>
            {view.failure !== undefined && (
              <p className="omfp-thread-header__failure" data-testid="thread-failure">
                {view.failure}
              </p>
            )}
            <div className="omfp-thread-header__actions">
              <Button
                size="sm"
                icon={<Icon name="pencil" />}
                data-testid="rename-thread"
                onPress={() => {
                  close();
                  setRenaming(threadId);
                }}
              >
                {text.rename}
              </Button>
              {compact && <ProcessAction threadId={threadId} view={view} onDone={close} />}
            </div>
            <Lifecycle threadId={threadId} archived={view.archived} onDone={close} />
            {compact && (
              <>
                <ThinkingSelect threadId={threadId} view={view} onChanged={onChanged} />
                <Chips view={view} onDone={close} />
                <div className="omfp-thread-header__actions">
                  <Usage threadId={threadId} view={view} />
                </div>
              </>
            )}
          </>
        )}
      </PopoverDialog>
    </DialogTrigger>
  );
}

/** `thread.state`, `thread.models` and `thread.thinking_levels` while the process runs, read again after a login or
 * logout of this window (`AUTH_REVISION`, UI-385); the function refreshes the session after a model or thinking
 * change. */
function useThreadDetails(threadId: string, running: boolean): [ThreadDetails, () => void] {
  const call = useCall();
  const [details, setDetails] = useState<ThreadDetails>(NO_DETAILS);
  const [authRevision] = useWindowState(AUTH_REVISION);
  useEffect(() => {
    if (!running) {
      return;
    }
    // Read again on every login or logout.
    void authRevision;
    let live = true;
    void fetchDetails(call, threadId).then((fetched) => {
      if (live) {
        setDetails(fetched);
      }
    });
    return () => {
      live = false;
    };
  }, [call, threadId, running, authRevision]);
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
    modelsLoaded: models.status === "fulfilled",
    thinkingLevels: levels.status === "fulfilled" ? decodeThinkingLevels(levels.value) : [],
  };
}

/** The title, or its editor while renaming (`Space r`, `:rename`, the `⋯` popover's Rename). Saving or cancelling
 * leaves the focus on the title (UI-286). */
function Title({ threadId, view }: { threadId: string; view: HeaderView }) {
  const [renaming, setRenaming] = useWindowState(RENAMING_THREAD);
  const actions = useThreadActions();
  const title = useRef<HTMLHeadingElement>(null);
  const refocus = useRef(false);
  useEffect(() => {
    if (renaming !== threadId && refocus.current) {
      refocus.current = false;
      title.current?.focus();
    }
  }, [renaming, threadId]);
  const close = () => {
    refocus.current = true;
    setRenaming(undefined);
  };
  if (renaming !== threadId) {
    return (
      <ChatTitle
        ref={title}
        // A focus target only, after renaming (UI-286).
        tabIndex={-1}
        placeholder={view.placeholder}
        data-testid="thread-title"
        title={view.title}
      >
        {view.title}
      </ChatTitle>
    );
  }
  return (
    <InlineEditor
      testid="rename-form"
      inputTestId="title-input"
      label={text.renameLabel}
      initial={view.placeholder ? "" : view.title}
      onSave={(value) => {
        close();
        if (value !== "") {
          void actions.rename(threadId, value);
        }
      }}
      onCancel={close}
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

function Chips({ view, onDone }: { view: HeaderView; onDone?: () => void }): ReactNode {
  return (
    <div className="omfp-thread-header__chips">
      <WorkspaceChipButton chip={view.workspace} onDone={onDone} />
    </div>
  );
}

/**
 * The thread's workspace (UI-340, UI-484): a bordered button with its name and repository count, the repositories as
 * its tooltip and description; a press shows `omfp:workspace`, which lists them — its tab, or a floating panel when
 * the view lacks it — after `onDone` (closing the compact header's popover).
 */
function WorkspaceChipButton({
  chip,
  onDone,
}: {
  chip: WorkspaceChip | undefined;
  onDone?: (() => void) | undefined;
}): ReactNode {
  const [, reveal] = useWindowState(REVEAL_WIDGET);
  const reposId = useId();
  if (chip === undefined) {
    return null;
  }
  return (
    <>
      <Button
        size="sm"
        icon={<Icon name="folder" />}
        className="omfp-thread-header__workspace"
        data-testid="workspace-chip"
        data-repos={chip.repoCount}
        aria-describedby={reposId}
        onPress={() => {
          onDone?.();
          reveal({ reference: "omfp:workspace", nonce: Date.now() });
        }}
      >
        <span className="omfp-thread-header__workspace-label" title={chip.tooltip}>
          {chip.label}
        </span>
      </Button>
      <span id={reposId} hidden>
        {chip.tooltip}
      </span>
    </>
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

/** The model list; when omp offers no model, Log in instead (UI-388). */
function ModelSelect({ threadId, view, onChanged }: SessionProps): ReactNode {
  const call = useCall();
  const host = useHost();
  const change = useSessionChange(onChanged);
  const onModel = (key: Key | null) => {
    const model = splitModel(String(key ?? ""));
    if (model !== undefined && String(key) !== view.model) {
      change(call("thread.set_model", { threadId, ...model }));
    }
  };
  if (view.noModels) {
    return (
      <span className="omfp-thread-header__no-models" data-testid="no-models">
        {text.noModels}
        <Button
          size="sm"
          variant="primary"
          aria-description={text.logInHint}
          onPress={() => requestLogin(host)}
          data-testid="models-login"
        >
          {text.logIn}
        </Button>
      </span>
    );
  }
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

/** Log in when omp needs one (UI-388), and stop/restart; `onDone` runs after pressing one (closes the compact header's
 * popover). Why the process failed is the state's tooltip and the `⋯` popover's first line (UI-482). */
function ProcessAction({ threadId, view, onDone }: { threadId: string; view: HeaderView; onDone?: () => void }) {
  const actions = useThreadActions();
  const host = useHost();
  return (
    <>
      {view.login && (
        <Button
          size="sm"
          variant="primary"
          aria-description={text.logInHint}
          data-testid="header-login"
          onPress={() => {
            onDone?.();
            requestLogin(host);
          }}
        >
          {text.logIn}
        </Button>
      )}
      {view.action !== undefined && (
        <Button
          size="sm"
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

/** The restart the daemon holds back, named by its reasons (UI-349: "Reload to load MCP tools", "Restart to load new
 * models"…): `thread.mcp_reconnect` restarts the thread's omp now; a refusal (`busy` while the turn runs) goes to the
 * error toasts. */
function PendingRestart({ threadId, view }: { threadId: string; view: HeaderView }): ReactNode {
  const call = useCall();
  const report = useReportError();
  const [pending, setPending] = useState(false);
  if (view.restart === undefined) {
    return null;
  }
  return (
    <Button
      size="sm"
      variant="primary"
      icon={<Icon name="refresh-cw" />}
      isPending={pending}
      aria-description={view.restart.hint}
      data-testid="restart-pending"
      onPress={() => {
        setPending(true);
        call("thread.mcp_reconnect", { threadId })
          .catch(report)
          .finally(() => setPending(false));
      }}
    >
      {view.restart.label}
    </Button>
  );
}

/** Archive (unarchive when archived) and delete, which asks for confirmation first (UI-026). */
function Lifecycle({ threadId, archived, onDone }: { threadId: string; archived: boolean; onDone?: () => void }) {
  const actions = useThreadActions();
  return (
    <div className="omfp-thread-header__lifecycle">
      <Button
        size="sm"
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
};

/** The thread's state (UI-157, UI-483): glyph and word without a box, popping when it changes after the header opened;
 * why the process failed as its tooltip and accessible description. */
function HeaderState({ status, failure }: { status: ThreadStatus; failure: string | undefined }): ReactNode {
  const first = useRef(status);
  return (
    <StateBadge
      key={status}
      state={BADGE_STATE[status]}
      fresh={status !== first.current}
      bare
      title={failure}
      aria-description={failure}
      data-testid="thread-state"
      data-status={status}
    >
      {statusLabel(status)}
    </StateBadge>
  );
}

/**
 * `N in background` while jobs or subagents of the thread run (UI-274); a press shows `omfp:background` — its tab, or
 * a floating panel when the view lacks it. Its own component: only it follows the thread's events.
 */
function BackgroundBadge({ threadId }: { threadId: string }): ReactNode {
  const running = useRunningInBackground(threadId);
  const [, reveal] = useWindowState(REVEAL_WIDGET);
  if (running === 0) {
    return null;
  }
  return (
    <Button
      size="sm"
      data-testid="background-badge"
      data-count={running}
      onPress={() => reveal({ reference: "omfp:background", nonce: Date.now() })}
    >
      {text.inBackground(running)}
    </Button>
  );
}
