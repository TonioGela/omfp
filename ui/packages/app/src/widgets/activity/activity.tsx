/**
 * `omfp:activity` (UI-241): the activity log of agent-originated actions (`activity.list`, PRT-086, AGT-072), newest
 * first: when, which thread's agent (a button selecting it), the method, what its params name, and the error of a
 * failed call. Newer entries are asked for on the host's 2 s poll (the one `useFile` follows); "Load older" pages
 * back. "This thread" keeps the selected thread's entries; the prop `threadId` fixes the thread.
 */

import { Button, EmptyState } from "omfp/kit";
import {
  SELECTED_THREAD,
  strings,
  useCall,
  useHost,
  useReportError,
  useThreadActions,
  useThreads,
  useWindowState,
  useWorkspaces,
  type WidgetProps,
} from "omfp/sdk";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { type ActivityLog, activityRows, withNewer, withOlder } from "./rows.ts";

const text = strings.shell.activity;

export function ActivityWidget({ threadId: fixed }: WidgetProps): ReactNode {
  const call = useCall();
  const host = useHost();
  const report = useReportError();
  const actions = useThreadActions();
  const threads = useThreads();
  const workspaces = useWorkspaces();
  const [selected] = useWindowState(SELECTED_THREAD);
  const [onlySelected, setOnlySelected] = useState(false);
  const pinned = typeof fixed === "string" ? fixed : undefined;
  const threadId = pinned ?? (onlySelected ? selected : undefined);
  const [log, setLog] = useState<ActivityLog | undefined>(undefined);
  const latest = useRef(log);
  latest.current = log;

  useEffect(() => {
    let live = true;
    const base = threadId === undefined ? {} : { threadId };
    setLog(undefined);
    call("activity.list", base)
      .then((page) => {
        if (live) {
          setLog({ entries: page.entries, more: page.more });
        }
      })
      .catch(report);
    const stop = host.filePoll(() => {
      const current = latest.current;
      if (current === undefined) {
        return;
      }
      const newest = current.entries[0]?.seq;
      // A failed poll is tried again on the next one.
      call("activity.list", newest === undefined ? base : { ...base, after: newest })
        .then((page) => {
          if (live) {
            setLog((shown) => (shown === undefined ? shown : withNewer(shown, page)));
          }
        })
        .catch(() => undefined);
    });
    return () => {
      live = false;
      stop();
    };
  }, [call, host, report, threadId]);

  const rows = useMemo(
    () =>
      log === undefined ? [] : activityRows(log.entries, threads, workspaces, strings.shell.untitled, text.mcpClient),
    [log, threads, workspaces],
  );
  const older = () => {
    const oldest = log?.entries.at(-1)?.seq;
    if (oldest === undefined) {
      return;
    }
    call("activity.list", { ...(threadId === undefined ? {} : { threadId }), before: oldest })
      .then((page) => setLog((shown) => (shown === undefined ? shown : withOlder(shown, page))))
      .catch(report);
  };

  return (
    <section className="omfp-activity" data-testid="activity" aria-label={text.label}>
      {pinned === undefined && (
        <div className="omfp-activity__bar">
          <Button
            size="sm"
            variant="quiet"
            aria-pressed={onlySelected}
            isDisabled={!onlySelected && selected === undefined}
            data-testid="activity-this-thread"
            onPress={() => setOnlySelected((only) => !only)}
          >
            {text.thisThread}
          </Button>
        </div>
      )}
      {log === undefined ? (
        <p className="omfp-muted" data-testid="activity-loading">
          {text.loading}
        </p>
      ) : rows.length === 0 ? (
        <EmptyState icon="inbox" title={text.empty} data-testid="activity-empty">
          {text.emptyHint}
        </EmptyState>
      ) : (
        <ol className="omfp-activity__list">
          {rows.map((row) => (
            <li
              key={row.seq}
              className="omfp-activity__row"
              data-testid="activity-row"
              data-seq={row.seq}
              data-failed={row.error !== null || undefined}
            >
              <time className="omfp-activity__time" dateTime={new Date(row.at).toISOString()}>
                {new Date(row.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
              </time>
              {row.threadId === null ? (
                <span className="omfp-activity__thread" data-testid="activity-mcp">
                  {row.thread}
                </span>
              ) : (
                <button
                  type="button"
                  className="omfp-activity__thread"
                  data-testid="activity-thread"
                  aria-label={text.selectThread(row.thread)}
                  disabled={!row.known}
                  onClick={() => {
                    if (row.threadId !== null) {
                      actions.select(row.threadId);
                    }
                  }}
                >
                  {row.thread}
                </button>
              )}
              <span className="omfp-activity__method" data-testid="activity-method">
                {row.method}
              </span>
              {row.target !== undefined && (
                <span className="omfp-activity__target" data-testid="activity-target">
                  {row.target}
                </span>
              )}
              {row.error !== null && (
                <span className="omfp-activity__error" data-testid="activity-error">
                  {text.failed(row.error)}
                </span>
              )}
            </li>
          ))}
        </ol>
      )}
      {log?.more === true && (
        <Button size="sm" variant="quiet" data-testid="activity-older" onPress={older}>
          {text.older}
        </Button>
      )}
    </section>
  );
}
