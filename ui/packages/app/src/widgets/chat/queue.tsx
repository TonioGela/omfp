/** The queue chips above the composer (UI-062): the kit's queue list and chips (UI-445). */
import type { ThreadState } from "@omfp/core";
import { QueueChip, QueueList } from "omfp/kit";
import {
  type QueueChip as QueuedMessageView,
  type QueueOp,
  queueChips,
  queueOpApplied,
  settleQueueOp,
  strings,
  useAnnounce,
  useCall,
  useWindowState,
} from "omfp/sdk";
import { PENDING_QUEUE } from "./state.ts";

/** The queue from the last `queue_update`; actions mark a chip busy until the daemon answers (UI-062). */
export function QueueChips({ threadId, state }: { threadId: string; state: ThreadState }) {
  const [pending] = useWindowState(PENDING_QUEUE);
  const chips = queueChips(state.queue, threadId, pending);
  if (chips.length === 0) {
    return null;
  }
  return (
    <QueueList data-testid="queue" aria-label={strings.chat.queue.label}>
      {chips.map((chip, index) => (
        // Queue texts may repeat; the position tells them apart.
        // biome-ignore lint/suspicious/noArrayIndexKey: see above
        <QueuedMessage key={`${chip.queue}-${index}`} threadId={threadId} chip={chip} />
      ))}
    </QueueList>
  );
}

function QueuedMessage({ threadId, chip }: { threadId: string; chip: QueuedMessageView }) {
  const call = useCall();
  const announce = useAnnounce();
  const [, setPending] = useWindowState(PENDING_QUEUE);
  const run = (op: QueueOp) => {
    const sent = { threadId, op, message: chip.text };
    setPending((previous) => [...previous, sent]);
    const method = op === "remove" ? "thread.queue.remove" : "thread.queue.promote";
    const queue = chip.queue === "steering" ? "steering" : "followUp";
    const settle = (applied: boolean) => {
      setPending((previous) => settleQueueOp(previous, sent));
      if (!applied) {
        announce(strings.chat.queue.gone(chip.text));
      }
    };
    call(method, { threadId, message: chip.text, queue }).then(
      (result) => settle(queueOpApplied(op, result)),
      () => settle(false),
    );
  };
  const busy = chip.pending !== undefined;
  return (
    <QueueChip
      kind={chip.queue === "steering" ? "steer" : "follow-up"}
      label={strings.chat.queue.kind[chip.queue]}
      busy={busy}
      data-testid="queue-chip"
      data-queue={chip.queue}
      data-pending={chip.pending}
      promoteLabel={strings.chat.queue.promote}
      removeLabel={strings.chat.queue.remove}
      {...(chip.canPromote ? { onPromote: () => run("promote") } : {})}
      onRemove={() => run("remove")}
    >
      {chip.text}
    </QueueChip>
  );
}
