/** One thread of a workspace section (UI-020): state mark, title, time, its actions and the spawned-by filter. */
import { Button, Icon, IconButton, StateMark } from "@omfp/kit";
import type { ThreadSummary } from "@omfp/protocol";
import type { ReactNode } from "react";
import { useThreadActions, useWindowState } from "../../sdk/index.ts";
import { strings } from "../../strings/index.ts";
import { displayTitle, statusLabel, threadStatus } from "../../vm/threads.ts";
import { relativeAge } from "./age.ts";
import { SPAWNED_BY } from "./keys.ts";
import { threadMarkState, useFresh } from "./marks.ts";

const text = strings.shell.threads;

export function ThreadRow({
  thread,
  selected,
  unread,
  snippet,
  spawned,
  archived = false,
}: {
  thread: ThreadSummary;
  selected: boolean;
  unread: boolean;
  snippet: string | undefined;
  /** How many threads it spawned, transitively; `undefined` for none (UI-240). */
  spawned: number | undefined;
  archived?: boolean;
}): ReactNode {
  const actions = useThreadActions();
  const [spawnedBy, setSpawnedBy] = useWindowState(SPAWNED_BY);
  const status = threadStatus(thread);
  const title = displayTitle(thread);
  const state = threadMarkState(status, unread);
  const fresh = useFresh(status);
  const filtering = spawnedBy === thread.id;
  return (
    <li
      className="omfp-row omfp-thread omfp-thread-item omfp-focus-item"
      data-state={state}
      data-unread={unread || undefined}
      data-selected={selected || undefined}
      data-fresh={fresh || undefined}
      data-archived={archived || undefined}
      data-snippet={snippet !== undefined || undefined}
    >
      <button
        type="button"
        className="omfp-thread-row omfp-focus-item__target"
        data-testid="thread-row"
        data-thread-id={thread.id}
        data-status={status}
        data-title-source={thread.titleSource}
        data-archived={archived ? "true" : undefined}
        aria-current={selected ? "true" : undefined}
        onClick={() => actions.select(thread.id)}
      >
        <StateMark state={state} label={statusLabel(status)} data-testid="thread-status" data-status={status} />
        <span className="omfp-row__title omfp-thread-row__title">{title}</span>
        {unread && (
          <span className="omfp-visually-hidden" data-testid="unread">
            {text.unread}
          </span>
        )}
        <span className="omfp-row__meta" data-testid="thread-age">
          {relativeAge(thread.updatedAt, Date.now())}
        </span>
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
      {spawned !== undefined && (
        <Button
          size="sm"
          variant="quiet"
          className="omfp-thread-row__spawned"
          icon={<Icon name="git-fork" />}
          aria-label={text.spawnedFilter(spawned, title)}
          aria-pressed={filtering}
          data-testid="spawned-filter"
          data-thread-id={thread.id}
          onPress={() => setSpawnedBy(filtering ? undefined : thread.id)}
        >
          {spawned}
        </Button>
      )}
      <IconButton
        className="omfp-row__delete"
        label={text.deleteThread(title)}
        icon={<Icon name="x" />}
        data-testid="row-delete-thread"
        data-thread-id={thread.id}
        onPress={() => actions.requestDelete(thread.id)}
      />
    </li>
  );
}
