/**
 * `omfp:header`, the selected thread's header (UI-030): title with rename in place, tags, the workspace and
 * repository chips, model and thinking selectors, context usage, session cost, the process state with stop/restart,
 * and archive/unarchive/delete (UI-026; delete asks first, in the frame's confirmation dialog).
 */

import { Button, type Key, Select, TextField } from "@omfp/kit";
import type { ThreadSummary } from "@omfp/protocol";
import { type FormEvent, type KeyboardEvent, type ReactNode, useEffect, useMemo, useState } from "react";
import {
  RENAMING_THREAD,
  type SdkClient,
  SELECTED_THREAD,
  useCall,
  useReportError,
  useThread,
  useThreadActions,
  useThreadSummary,
  useWindowState,
  useWorkspaces,
  type WidgetProps,
} from "../../sdk/index.ts";
import { strings } from "../../strings/index.ts";
import {
  decodeModels,
  decodeSession,
  decodeThinkingLevels,
  type HeaderView,
  headerView,
  NO_DETAILS,
  parseTags,
  splitModel,
  type ThreadDetails,
} from "../../vm/header.ts";
import { statusLabel } from "../../vm/threads.ts";
import { StatusBadge, WorkspaceChipView } from "../threads/index.ts";

const text = strings.shell.header;

export function HeaderWidget(_props: WidgetProps): ReactNode {
  const [selected] = useWindowState(SELECTED_THREAD);
  const summary = useThreadSummary(selected);
  if (summary === undefined) {
    return (
      <header className="omfp-header" data-testid="thread-header" data-empty="true">
        <p className="omfp-muted" data-testid="header-empty">
          {text.noThread}
        </p>
      </header>
    );
  }
  return <ThreadHeader key={summary.id} summary={summary} />;
}

function ThreadHeader({ summary }: { summary: ThreadSummary }): ReactNode {
  const thread = useThread(summary.id);
  const workspaces = useWorkspaces();
  const list = useMemo(() => [...workspaces.values()], [workspaces]);
  const [details, refreshSession] = useThreadDetails(summary.id, summary.process.state === "running");
  const view = headerView(summary, thread?.state, details, list);
  return (
    <header className="omfp-header" data-testid="thread-header" data-thread-id={summary.id} data-omfp-scope="header">
      <Title threadId={summary.id} view={view} />
      <Tags threadId={summary.id} tags={view.tags} />
      <Chips view={view} />
      <Session threadId={summary.id} view={view} onChanged={refreshSession} />
      <Process threadId={summary.id} view={view} />
      <Lifecycle threadId={summary.id} archived={view.archived} />
    </header>
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

/** The title, or its editor while renaming (`Space r`, `:rename`). */
function Title({ threadId, view }: { threadId: string; view: HeaderView }): ReactNode {
  const [renaming, setRenaming] = useWindowState(RENAMING_THREAD);
  const actions = useThreadActions();
  if (renaming !== threadId) {
    return (
      <div className="omfp-header__title">
        <h1 data-testid="thread-title" data-placeholder={view.placeholder ? "true" : undefined}>
          {view.title}
        </h1>
        <Button size="sm" variant="quiet" data-testid="rename-thread" onPress={() => setRenaming(threadId)}>
          {text.rename}
        </Button>
      </div>
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

function Tags({ threadId, tags }: { threadId: string; tags: readonly string[] }): ReactNode {
  const [editing, setEditing] = useState(false);
  const actions = useThreadActions();
  if (editing) {
    return (
      <InlineEditor
        testid="tags-form"
        inputTestId="tags-input"
        label={text.tagsLabel}
        initial={tags.join(", ")}
        onSave={(value) => {
          setEditing(false);
          void actions.setTags(threadId, parseTags(value));
        }}
        onCancel={() => setEditing(false)}
      />
    );
  }
  return (
    <div className="omfp-header__tags">
      <ul aria-label={text.tags}>
        {tags.map((tag) => (
          <li key={tag} className="omfp-tag" data-testid="thread-tag">
            {tag}
          </li>
        ))}
      </ul>
      <Button size="sm" variant="quiet" data-testid="edit-tags" onPress={() => setEditing(true)}>
        {text.editTags}
      </Button>
    </div>
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
    <form className="omfp-header__editor" data-testid={testid} onSubmit={submit}>
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
    <div className="omfp-header__chips">
      {view.chips.map((chip) => (
        <WorkspaceChipView key={chip.key} chip={chip} />
      ))}
    </div>
  );
}

/** Model and thinking selectors, context usage and session cost. */
function Session({ threadId, view, onChanged }: { threadId: string; view: HeaderView; onChanged: () => void }) {
  const call = useCall();
  const report = useReportError();
  const change = (request: Promise<unknown>) => {
    request.then(onChanged, report);
  };
  const onModel = (key: Key | null) => {
    const model = splitModel(String(key ?? ""));
    if (model !== undefined && String(key) !== view.model) {
      change(call("thread.set_model", { threadId, ...model }));
    }
  };
  const onThinking = (key: Key | null) => {
    const level = String(key ?? "");
    if (view.thinkingLevels.includes(level) && level !== view.thinking) {
      change(call("thread.set_thinking", { threadId, level }));
    }
  };
  return (
    <div className="omfp-header__session">
      <Select
        label={text.model}
        hideLabel
        placeholder={text.modelPlaceholder}
        data-testid="model-select"
        items={view.models.map((model) => ({ id: model.value, label: model.label }))}
        selectedKey={view.model ?? null}
        onSelectionChange={onModel}
      />
      <Select
        label={text.thinking}
        hideLabel
        placeholder={text.thinkingPlaceholder}
        data-testid="thinking-select"
        items={view.thinkingLevels.map((level) => ({ id: level, label: level }))}
        selectedKey={view.thinking ?? null}
        onSelectionChange={onThinking}
      />
      {view.context !== undefined && (
        <span className="omfp-meter" data-testid="context-usage" title={text.contextUsage}>
          {view.context}
        </span>
      )}
      {view.cost !== undefined && (
        <span className="omfp-meter" data-testid="session-cost" title={text.sessionCost}>
          {view.cost}
        </span>
      )}
    </div>
  );
}

/** Process state, failure reason, stop/restart. */
function Process({ threadId, view }: { threadId: string; view: HeaderView }): ReactNode {
  const actions = useThreadActions();
  return (
    <div className="omfp-header__process">
      <StatusBadge status={view.status} testid="thread-state" />
      <span className="omfp-header__state">{statusLabel(view.status)}</span>
      {view.failure !== undefined && (
        <span className="omfp-header__failure" data-testid="thread-failure">
          {view.failure}
        </span>
      )}
      {view.action !== undefined && (
        <Button
          size="sm"
          data-testid="process-action"
          data-action={view.action}
          onPress={() => void (view.action === "stop" ? actions.stop(threadId) : actions.restart(threadId))}
        >
          {view.action === "stop" ? text.stop : text.restart}
        </Button>
      )}
    </div>
  );
}

/** Archive (unarchive when archived) and delete, which asks for confirmation first (UI-026). */
function Lifecycle({ threadId, archived }: { threadId: string; archived: boolean }): ReactNode {
  const actions = useThreadActions();
  return (
    <div className="omfp-header__lifecycle">
      <Button
        size="sm"
        data-testid="archive-thread"
        onPress={() => void (archived ? actions.unarchive(threadId) : actions.archive(threadId))}
      >
        {archived ? text.unarchive : text.archive}
      </Button>
      <Button size="sm" variant="danger" data-testid="delete-thread" onPress={() => actions.requestDelete(threadId)}>
        {text.delete}
      </Button>
    </div>
  );
}
