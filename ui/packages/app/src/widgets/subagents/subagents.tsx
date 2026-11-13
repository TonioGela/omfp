/**
 * `omfp:subagents` (UI-081): the selected thread's subagents — from its state's subagent frames, plus the running ones
 * `thread.subagents` reports — with their status, task, current activity and use. Opening one shows its transcript in a
 * box titled on its border (UI-187), read incrementally with `thread.subagent_messages` (`fromByte`) as its progress
 * changes; a running one can be steered (`thread.subagent_steer`) and cancelled (`thread.subagent_cancel`).
 */

import { BorderBox, Button, EmptyState, Icon, IconButton, Markdown, Message, StateMark, TextField } from "omfp/kit";
import {
  appendTranscriptPage,
  EMPTY_TRANSCRIPT,
  OPEN_SUBAGENT,
  SELECTED_THREAD,
  type SubagentRow,
  type SubagentTranscript,
  strings,
  subagentLines,
  useCall,
  useReportError,
  useSubagentRows,
  useWindowState,
  type WidgetProps,
} from "omfp/sdk";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";

const text = strings.shell.subagents;

export function SubagentsWidget(_props: WidgetProps): ReactNode {
  const [threadId] = useWindowState(SELECTED_THREAD);
  if (threadId === undefined) {
    return <EmptyState icon="inbox" title={text.noThread} data-testid="subagents-no-thread" />;
  }
  return <Subagents key={threadId} threadId={threadId} />;
}

/** The state mark of a subagent status. */
function mark(status: string) {
  switch (status) {
    case "completed":
      return "done";
    case "failed":
      return "failed";
    case "aborted":
      return "idle";
    default:
      return "running";
  }
}

function Subagents({ threadId }: { threadId: string }): ReactNode {
  const { rows, refresh } = useSubagentRows(threadId);
  const [open, setOpen] = useState<string | undefined>(undefined);
  // `omfp:background` opens a subagent here (UI-271).
  const [request] = useWindowState(OPEN_SUBAGENT);
  useEffect(() => {
    if (request !== undefined && request.threadId === threadId) {
      setOpen(request.subagentId);
    }
  }, [request, threadId]);
  const opened = rows.find((row) => row.id === open);

  return (
    <section className="omfp-subagents" data-testid="subagents" aria-label={text.label}>
      <div className="omfp-subagents__bar">
        <IconButton
          label={text.refresh}
          icon={<Icon name="refresh-cw" />}
          onPress={refresh}
          data-testid="subagents-refresh"
        />
      </div>
      {rows.length === 0 ? (
        <EmptyState icon="inbox" title={text.none} data-testid="subagents-none">
          {text.noneHint}
        </EmptyState>
      ) : (
        <ul className="omfp-subagents__list">
          {rows.map((row) => (
            <li key={row.id}>
              <button
                type="button"
                className="omfp-subagents__row omfp-focus-item"
                data-testid="subagent-row"
                data-subagent={row.id}
                data-status={row.status}
                aria-expanded={open === row.id}
                aria-label={text.open(row.id)}
                onClick={() => setOpen((current) => (current === row.id ? undefined : row.id))}
              >
                <StateMark state={mark(row.status)} />
                <span className="omfp-subagents__name">{row.id}</span>
                <span className="omfp-subagents__agent">{row.agent}</span>
                <span className="omfp-subagents__status">{text.status[row.status] ?? row.status}</span>
                <span className="omfp-subagents__task">{row.description ?? row.task}</span>
                {row.activity !== undefined && <span className="omfp-subagents__activity">{row.activity}</span>}
                {row.meta !== "" && <span className="omfp-subagents__meta">{row.meta}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
      {opened !== undefined && (
        <Transcript
          key={opened.id}
          threadId={threadId}
          row={opened}
          version={`${opened.status}:${opened.meta}:${opened.activity ?? ""}`}
          onClose={() => setOpen(undefined)}
        />
      )}
    </section>
  );
}

interface TranscriptProps {
  readonly threadId: string;
  readonly row: SubagentRow;
  /** Changes with the subagent's progress: the transcript reads what is new. */
  readonly version: string;
  readonly onClose: () => void;
}

function Transcript({ threadId, row, version, onClose }: TranscriptProps): ReactNode {
  const call = useCall();
  const reportError = useReportError();
  const [transcript, setTranscript] = useState<SubagentTranscript | undefined>(undefined);
  const [requested, setRequested] = useState(0);
  const subagentId = row.id;

  // Where the next read starts: the last page's `nextByte`.
  const nextByte = useRef(0);
  useEffect(() => {
    nextByte.current = transcript?.nextByte ?? 0;
  }, [transcript]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `version` and `requested` trigger a read of what is new.
  useEffect(() => {
    let current = true;
    call("thread.subagent_messages", { threadId, subagentId, fromByte: nextByte.current }).then((page) => {
      if (current) {
        setTranscript((latest) => appendTranscriptPage(latest ?? EMPTY_TRANSCRIPT, page));
      }
    }, reportError);
    return () => {
      current = false;
    };
  }, [call, reportError, threadId, subagentId, version, requested]);

  const lines = useMemo(() => (transcript === undefined ? [] : subagentLines(transcript.messages)), [transcript]);
  return (
    <BorderBox
      title={text.transcript(row.id)}
      level={2}
      className="omfp-subagents__transcript"
      data-testid="subagent-transcript"
      end={<IconButton label={text.close} icon={<Icon name="x" />} onPress={onClose} data-testid="subagent-close" />}
    >
      {transcript === undefined ? (
        <p className="omfp-subagents__empty">{text.loading}</p>
      ) : lines.length === 0 ? (
        <p className="omfp-subagents__empty">{text.empty}</p>
      ) : (
        <div className="omfp-subagents__lines">
          {lines.map((line, index) =>
            line.role === "tool" ? (
              // biome-ignore lint/suspicious/noArrayIndexKey: transcript lines only ever grow at the end
              <p key={index} className="omfp-subagents__tool" data-testid="subagent-line" data-role="tool">
                {line.text}
              </p>
            ) : (
              <Message
                // biome-ignore lint/suspicious/noArrayIndexKey: transcript lines only ever grow at the end
                key={index}
                author={line.role}
                name={line.role === "user" ? text.roleUser : text.roleAssistant}
                data-testid="subagent-line"
                data-role={line.role}
              >
                <Markdown text={line.text} />
              </Message>
            ),
          )}
        </div>
      )}
      <div className="omfp-subagents__actions">
        <Button
          size="sm"
          variant="quiet"
          icon={<Icon name="refresh-cw" />}
          onPress={() => setRequested((count) => count + 1)}
          data-testid="subagent-more"
        >
          {text.more}
        </Button>
        {row.live && <Steer threadId={threadId} row={row} />}
      </div>
    </BorderBox>
  );
}

function Steer({ threadId, row }: { threadId: string; row: SubagentRow }): ReactNode {
  const call = useCall();
  const reportError = useReportError();
  const [message, setMessage] = useState("");
  const subagentId = row.id;
  const steer = useCallback(() => {
    const trimmed = message.trim();
    if (trimmed === "") {
      return;
    }
    call("thread.subagent_steer", { threadId, subagentId, text: trimmed }).then(() => setMessage(""), reportError);
  }, [call, reportError, threadId, subagentId, message]);
  return (
    <form
      className="omfp-subagents__steer"
      onSubmit={(event) => {
        event.preventDefault();
        steer();
      }}
    >
      <TextField
        label={text.steerLabel(row.id)}
        hideLabel
        placeholder={text.steerPlaceholder}
        value={message}
        onChange={setMessage}
        inputProps={{ "data-testid": "subagent-steer-input" }}
      />
      <Button size="sm" type="submit" icon={<Icon name="arrow-up" />} data-testid="subagent-steer">
        {text.steer}
      </Button>
      <Button
        size="sm"
        variant="danger"
        icon={<Icon name="square" />}
        aria-label={text.cancelLabel(row.id)}
        onPress={() => {
          call("thread.subagent_cancel", { threadId, subagentId }).catch(reportError);
        }}
        data-testid="subagent-cancel"
      >
        {text.cancel}
      </Button>
    </form>
  );
}
