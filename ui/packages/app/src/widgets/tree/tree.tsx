/**
 * `omfp:tree` (UI-083): the selected thread's session tree (`thread.tree`) in a box titled on its border (UI-187): one
 * row per user or assistant message and summary, the branch the transcript shows marked, the leaf saying so. A row's
 * actions go there (`thread.navigate`, optionally summarizing the branch left), fork a new session from it
 * (`thread.fork`) or label it (`thread.label`). A navigation to a user message gives its text back: it goes into the
 * composer (the thread's draft). The tree is read again after each settled turn and each change of session; a stopped
 * thread's tree is read on request, since reading it starts omp (DMN-070).
 */

import { BorderBox, Button, EmptyState, Icon, IconButton, TextField } from "omfp/kit";
import {
  draftKey,
  navigateOutcome,
  SELECTED_THREAD,
  strings,
  type TreeRow,
  treeView,
  useAnnounce,
  useCall,
  useReportError,
  useThread,
  useWindowState,
  type WidgetProps,
  withEditorText,
} from "omfp/sdk";
import { type CSSProperties, type ReactNode, useCallback, useEffect, useMemo, useState } from "react";

const text = strings.shell.tree;

export function TreeWidget(_props: WidgetProps): ReactNode {
  const [threadId] = useWindowState(SELECTED_THREAD);
  if (threadId === undefined) {
    return <EmptyState icon="git-branch" title={text.noThread} data-testid="tree-no-thread" />;
  }
  return <SessionTree key={threadId} threadId={threadId} />;
}

const GLYPH: Readonly<Record<TreeRow["kind"], string>> = {
  user: text.roleUser,
  assistant: text.roleAssistant,
  summary: text.roleSummary,
};

function SessionTree({ threadId }: { threadId: string }): ReactNode {
  const state = useThread(threadId)?.state;
  const call = useCall();
  const reportError = useReportError();
  const announce = useAnnounce();
  const [, setDraft] = useWindowState(draftKey(threadId));
  const [result, setResult] = useState<unknown>(undefined);
  const [requested, setRequested] = useState(0);
  const running = state?.process.state === "running";
  const settled = state?.settled ?? true;
  // Moves of the session that change the tree: a settled turn, a fork or navigation (a new history).
  const version = `${state?.session?.session_id ?? ""}:${state?.transcript.length ?? 0}:${settled}`;

  // biome-ignore lint/correctness/useExhaustiveDependencies: `version` triggers a new read after each move of the session.
  useEffect(() => {
    if (!running && requested === 0) {
      return;
    }
    let current = true;
    call("thread.tree", { threadId }).then((tree) => {
      if (current) {
        setResult(tree);
      }
    }, reportError);
    return () => {
      current = false;
    };
  }, [call, reportError, threadId, running, requested, version]);

  const refresh = useCallback(() => setRequested((count) => count + 1), []);
  const view = useMemo(() => treeView(result), [result]);
  const [labelling, setLabelling] = useState<string | undefined>(undefined);

  const navigate = useCallback(
    (row: TreeRow, summarize: boolean) => {
      call("thread.navigate", { threadId, entryId: row.id, summarize }).then((answer) => {
        const outcome = navigateOutcome(answer);
        if (outcome.kind === "unchanged") {
          announce(text.announce.cancelled);
        } else if (outcome.editorText === undefined) {
          announce(text.announce.navigated);
        } else {
          const restored = outcome.editorText;
          setDraft((draft) => ({ ...draft, text: withEditorText(draft.text, restored) }));
          announce(text.announce.editorText);
        }
        refresh();
      }, reportError);
    },
    [call, threadId, announce, setDraft, refresh, reportError],
  );

  const fork = useCallback(
    (row: TreeRow) => {
      call("thread.fork", { threadId, entryId: row.id }).then((answer) => {
        announce(navigateOutcome(answer).kind === "unchanged" ? text.announce.cancelled : text.announce.forked);
        refresh();
      }, reportError);
    },
    [call, threadId, announce, refresh, reportError],
  );

  const label = useCallback(
    (row: TreeRow, value: string) => {
      setLabelling(undefined);
      const trimmed = value.trim();
      call("thread.label", { threadId, entryId: row.id, ...(trimmed === "" ? {} : { label: trimmed }) }).then(
        refresh,
        reportError,
      );
    },
    [call, threadId, refresh, reportError],
  );

  if (result === undefined && !running) {
    return (
      <EmptyState
        icon="git-branch"
        title={text.label}
        data-testid="tree-stopped"
        action={
          <Button size="sm" icon={<Icon name="refresh-cw" />} onPress={refresh} data-testid="tree-load">
            {text.refresh}
          </Button>
        }
      />
    );
  }
  return (
    <BorderBox
      title={text.label}
      level={2}
      className="omfp-tree"
      data-testid="tree"
      end={
        <IconButton
          label={text.refresh}
          icon={<Icon name="refresh-cw" />}
          onPress={refresh}
          data-testid="tree-refresh"
        />
      }
    >
      {result === undefined ? (
        <p className="omfp-tree__empty">{text.loading}</p>
      ) : view.rows.length === 0 ? (
        <p className="omfp-tree__empty">{text.empty}</p>
      ) : (
        <ul className="omfp-tree__rows">
          {view.rows.map((row) => (
            <li
              key={row.id}
              className="omfp-tree__row"
              data-testid="tree-row"
              data-entry={row.id}
              data-kind={row.kind}
              data-active={row.active || undefined}
              data-leaf={row.leaf || undefined}
              style={{ "--depth": row.depth } as CSSProperties}
            >
              <span className="omfp-tree__glyph" aria-hidden="true">
                {GLYPH[row.kind]}
              </span>
              <span className="omfp-tree__text">{row.text}</span>
              {row.label !== undefined && (
                <span className="omfp-tree__label" data-testid="tree-label">
                  <Icon name="pencil" />
                  {row.label}
                </span>
              )}
              {row.leaf && <span className="omfp-tree__here">{text.leaf}</span>}
              {labelling === row.id ? (
                <LabelField row={row} onSave={label} onCancel={() => setLabelling(undefined)} />
              ) : (
                <span className="omfp-tree__actions">
                  {!row.leaf && (
                    <Button
                      size="sm"
                      variant="quiet"
                      isDisabled={!settled}
                      aria-label={text.navigateLabel(row.text)}
                      onPress={() => navigate(row, false)}
                      data-testid="tree-navigate"
                    >
                      {text.navigate}
                    </Button>
                  )}
                  {!row.active && (
                    <Button
                      size="sm"
                      variant="quiet"
                      isDisabled={!settled}
                      aria-label={text.summarizeLabel(row.text)}
                      onPress={() => navigate(row, true)}
                      data-testid="tree-summarize"
                    >
                      {text.summarize}
                    </Button>
                  )}
                  {row.forkable && (
                    <Button
                      size="sm"
                      variant="quiet"
                      icon={<Icon name="git-fork" />}
                      isDisabled={!settled}
                      aria-label={text.forkLabel(row.text)}
                      onPress={() => fork(row)}
                      data-testid="tree-fork"
                    >
                      {text.fork}
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="quiet"
                    icon={<Icon name="pencil" />}
                    aria-label={text.labelLabel(row.text)}
                    onPress={() => setLabelling(row.id)}
                    data-testid="tree-label-edit"
                  >
                    {text.labelAction}
                  </Button>
                </span>
              )}
            </li>
          ))}
        </ul>
      )}
    </BorderBox>
  );
}

function LabelField({
  row,
  onSave,
  onCancel,
}: {
  row: TreeRow;
  onSave: (row: TreeRow, value: string) => void;
  onCancel: () => void;
}): ReactNode {
  const [value, setValue] = useState(row.label ?? "");
  return (
    <form
      className="omfp-tree__label-form"
      onSubmit={(event) => {
        event.preventDefault();
        onSave(row, value);
      }}
    >
      <TextField
        label={text.labelField}
        hideLabel
        placeholder={text.labelField}
        value={value}
        onChange={setValue}
        autoFocus
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            onCancel();
          }
        }}
        inputProps={{ "data-testid": "tree-label-input" }}
      />
      <Button size="sm" type="submit" data-testid="tree-label-save">
        {text.save}
      </Button>
    </form>
  );
}
