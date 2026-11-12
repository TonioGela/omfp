/** The queue chips above the composer (UI-062). */
import type { ThreadState } from "@omfp/core";
import { Button, Icon, IconButton } from "omfp/kit";
import {
  type QueueChip,
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
    <ul className="omfp-queue" data-testid="queue" aria-label={strings.chat.queue.label}>
      {chips.map((chip, index) => (
        // Queue texts may repeat; the position tells them apart.
        // biome-ignore lint/suspicious/noArrayIndexKey: see above
        <QueueChipView key={`${chip.queue}-${index}`} threadId={threadId} chip={chip} />
      ))}
    </ul>
  );
}

function QueueChipView({ threadId, chip }: { threadId: string; chip: QueueChip }) {
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
    <li
      className="omfp-queue-chip omfp-queue__chip"
      data-kind={chip.queue === "steering" ? "steer" : "follow-up"}
      data-testid="queue-chip"
      data-queue={chip.queue}
      data-pending={chip.pending}
      aria-busy={busy}
    >
      <span className="omfp-queue-chip__kind">{strings.chat.queue.kind[chip.queue]}</span>
      <span className="omfp-queue-chip__text">{chip.text}</span>
      {chip.canPromote && (
        <Button size="sm" variant="quiet" data-testid="queue-promote" isDisabled={busy} onPress={() => run("promote")}>
          {strings.chat.queue.promote}
        </Button>
      )}
      <IconButton
        label={strings.chat.queue.remove}
        icon={<Icon name="x" />}
        data-testid="queue-remove"
        isDisabled={busy}
        onPress={() => run("remove")}
      />
    </li>
  );
}

/** The key chip of a send mode's key (UI-148): `shift+enter` steers while the agent works, `enter` sends otherwise. */
