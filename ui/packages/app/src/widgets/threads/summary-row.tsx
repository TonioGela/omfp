/**
 * One thread of a workspace section (UI-020): the kit's thread row (UI-169, UI-442) for a thread summary, with its
 * unarchive action and the spawned-by filter (UI-240).
 */
import { Button, Icon, ThreadRow } from "@omfp/kit";
import type { ThreadSummary } from "@omfp/protocol";
import type { ReactNode } from "react";
import { useThreadActions, useWindowState } from "../../sdk/index.ts";
import { strings } from "../../strings/index.ts";
import { displayTitle, statusLabel, threadStatus } from "../../vm/threads.ts";
import { relativeAge } from "./age.ts";
import { SPAWNED_BY } from "./keys.ts";
import { threadMarkState, useFresh } from "./marks.ts";

const text = strings.shell.threads;

export function SummaryRow({
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
  const fresh = useFresh(status);
  const filtering = spawnedBy === thread.id;
  return (
    <ThreadRow
      state={threadMarkState(status, unread)}
      stateLabel={statusLabel(status)}
      markProps={{ "data-status": status }}
      title={title}
      placeholder={thread.titleSource === "placeholder"}
      meta={relativeAge(thread.updatedAt, Date.now())}
      selected={selected}
      fresh={fresh}
      unread={unread}
      unreadLabel={text.unread}
      archived={archived}
      snippet={snippet}
      onSelect={() => actions.select(thread.id)}
      selectProps={{
        "data-testid": "thread-row",
        "data-thread-id": thread.id,
        "data-status": status,
        "data-title-source": thread.titleSource,
        "data-archived": archived ? "true" : undefined,
      }}
      actions={
        <>
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
              className="omfp-threads__spawned-filter"
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
        </>
      }
      onDelete={() => actions.requestDelete(thread.id)}
      deleteLabel={text.deleteThread(title)}
      deleteProps={{ "data-testid": "row-delete-thread", "data-thread-id": thread.id }}
    />
  );
}
