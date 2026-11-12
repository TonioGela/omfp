/**
 * `omfp:background` (UI-271..UI-273): the selected thread's background work — its omp background jobs (`bash` calls
 * that became jobs) and its subagents — with label, state, duration and last line. A job's row scrolls the chat to its
 * card; a subagent's opens it in `omfp:subagents`. Running subagents are cancelled with `thread.subagent_cancel`;
 * running jobs, which the RPC cannot kill, are stopped by a steer asking the agent to (it kills them via `proc://`).
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
import type { ReactNode } from "react";

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

/** The tool card of call `toolCallId` in the transcript. */
function cardOf(toolCallId: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-testid="tool-card"][data-call="${CSS.escape(toolCallId)}"]`);
}

function Background({ threadId }: { threadId: string }): ReactNode {
  const { rows, stop, cancel } = useBackground(threadId);
  const [, reveal] = useWindowState(REVEAL_WIDGET);
  const [, openSubagent] = useWindowState(OPEN_SUBAGENT);
  const show = (row: BackgroundRow) => {
    const nonce = Date.now();
    if (row.kind === "subagent") {
      openSubagent({ threadId, subagentId: row.id, nonce });
      reveal({ reference: "omfp:subagents", nonce });
      return;
    }
    const toolCallId = row.toolCallId ?? "";
    const scroll = () => cardOf(toolCallId)?.scrollIntoView?.({ block: "center" });
    if (cardOf(toolCallId) === null) {
      // The chat may sit in a tab not shown: show it, then scroll once it rendered.
      reveal({ reference: "omfp:chat", nonce });
      requestAnimationFrame(scroll);
    } else {
      scroll();
    }
  };
  return (
    <section className="omfp-subagents omfp-background" data-testid="background" aria-label={text.label}>
      {rows.length === 0 ? (
        <EmptyState icon="inbox" title={text.none} data-testid="background-none">
          {text.noneHint}
        </EmptyState>
      ) : (
        <ul className="omfp-subagents__list">
          {rows.map((row) => (
            <li key={`${row.kind}:${row.id}`} className="omfp-background__item">
              <button
                type="button"
                className="omfp-subagents__row"
                data-testid="background-row"
                data-kind={row.kind}
                data-id={row.id}
                data-status={row.state}
                aria-label={row.kind === "job" ? text.showJob(row.id) : text.openSubagent(row.id)}
                onClick={() => show(row)}
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
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
