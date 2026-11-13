/**
 * `omfp:background` (UI-271..UI-273, UI-490..UI-492): the selected thread's background work — its omp background jobs
 * (`bash` calls that became jobs) and its subagents — with label, state, duration and last line. A job's row scrolls
 * the chat to its card; a subagent's row opens its detail under it (`subagent.tsx`: transcript and steer), as does an
 * `OPEN_SUBAGENT` request. Running subagents are cancelled with `thread.subagent_cancel`; running jobs, which the RPC
 * cannot kill, are stopped by a steer asking the agent to (it kills them via `proc://`).
 */

import { Button, EmptyState, StateMark } from "omfp/kit";
import {
  type BackgroundRow,
  formatDuration,
  OPEN_SUBAGENT,
  REVEAL_WIDGET,
  SELECTED_THREAD,
  strings,
  useBackground,
  useWindowState,
  type WidgetProps,
} from "omfp/sdk";
import { type ReactNode, useEffect, useId, useState } from "react";
import { SubagentDetail } from "./subagent.tsx";

const text = strings.shell.background;

export function BackgroundWidget(_props: WidgetProps): ReactNode {
  const [threadId] = useWindowState(SELECTED_THREAD);
  if (threadId === undefined) {
    return <EmptyState icon="inbox" title={text.noThread} data-testid="background-no-thread" />;
  }
  return <Background key={threadId} threadId={threadId} />;
}

/** The state mark of a row. */
function mark(state: BackgroundRow["state"]) {
  switch (state) {
    case "completed":
      return "done";
    case "failed":
      return "failed";
    case "cancelled":
      return "idle";
    default:
      return "running";
  }
}

/**
 * The tool card of call `toolCallId` in the transcript the window shows: a chat in a tab not selected stays mounted,
 * hidden (UI-400), as does one in a kept view (UI-390).
 */
function cardOf(toolCallId: string): HTMLElement | undefined {
  const cards = document.querySelectorAll<HTMLElement>(
    `[data-testid="tool-card"][data-call="${CSS.escape(toolCallId)}"]`,
  );
  return [...cards].find((card) => card.checkVisibility());
}

function Background({ threadId }: { threadId: string }): ReactNode {
  const { rows, stop, cancel } = useBackground(threadId);
  const [, reveal] = useWindowState(REVEAL_WIDGET);
  // The subagent whose detail is open: one at a time, toggled by its row or opened by a request (UI-492).
  const [open, setOpen] = useState<string | undefined>(undefined);
  const [request] = useWindowState(OPEN_SUBAGENT);
  useEffect(() => {
    if (request !== undefined && request.threadId === threadId) {
      setOpen(request.subagentId);
    }
  }, [request, threadId]);
  const showJob = (row: BackgroundRow) => {
    const toolCallId = row.toolCallId ?? "";
    const scroll = () => cardOf(toolCallId)?.scrollIntoView?.({ block: "center" });
    if (cardOf(toolCallId) === undefined) {
      // The chat may sit in a tab not shown: show it, then scroll once it rendered.
      reveal({ reference: "omfp:chat", nonce: Date.now() });
      requestAnimationFrame(scroll);
    } else {
      scroll();
    }
  };
  return (
    <section className="omfp-subagents" data-testid="background" aria-label={text.label}>
      {rows.length === 0 ? (
        <EmptyState icon="inbox" title={text.none} data-testid="background-none">
          {text.noneHint}
        </EmptyState>
      ) : (
        <ul className="omfp-subagents__list">
          {rows.map((row) => (
            <Row
              key={`${row.kind}:${row.id}`}
              threadId={threadId}
              row={row}
              open={row.kind === "subagent" && open === row.id}
              onPress={() =>
                row.kind === "job" ? showJob(row) : setOpen((current) => (current === row.id ? undefined : row.id))
              }
              stop={stop}
              cancel={cancel}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

interface RowProps {
  readonly threadId: string;
  readonly row: BackgroundRow;
  /** A subagent's detail shows under its row. */
  readonly open: boolean;
  readonly onPress: () => void;
  readonly stop: (jobId: string) => void;
  readonly cancel: (subagentId: string) => void;
}

function Row({ threadId, row, open, onPress, stop, cancel }: RowProps): ReactNode {
  const detailId = useId();
  const subagent = row.subagent;
  return (
    <li className="omfp-background__item">
      <div className="omfp-background__head">
        <button
          type="button"
          className="omfp-subagents__row omfp-focus-item"
          data-testid="background-row"
          data-kind={row.kind}
          data-id={row.id}
          data-status={row.state}
          aria-label={row.kind === "job" ? text.showJob(row.id) : text.subagent(row.id)}
          aria-expanded={subagent === undefined ? undefined : open}
          aria-controls={open ? detailId : undefined}
          onClick={onPress}
        >
          <StateMark state={mark(row.state)} />
          <span className="omfp-subagents__name">{row.id}</span>
          <span className="omfp-subagents__status" data-testid="background-state">
            {row.stopRequested ? text.stopRequested : text.state[row.state]}
          </span>
          <span className="omfp-subagents__meta" data-testid="background-duration">
            {row.durationMs === undefined ? "" : formatDuration(row.durationMs)}
          </span>
          <span className="omfp-subagents__task" data-testid="background-label" title={row.label}>
            {row.label}
          </span>
          {row.lastLine !== undefined && (
            <span className="omfp-subagents__activity" data-testid="background-last-line" title={row.lastLine}>
              {row.lastLine}
            </span>
          )}
        </button>
        {row.state === "running" && row.kind === "job" && !row.stopRequested && (
          <Button size="sm" onPress={() => stop(row.id)} data-testid="background-stop">
            {text.stop}
          </Button>
        )}
        {row.state === "running" && row.kind === "subagent" && (
          <Button size="sm" onPress={() => cancel(row.id)} data-testid="background-cancel">
            {text.cancel}
          </Button>
        )}
      </div>
      {open && subagent !== undefined && <SubagentDetail id={detailId} threadId={threadId} row={subagent} />}
    </li>
  );
}
