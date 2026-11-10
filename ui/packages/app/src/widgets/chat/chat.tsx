/**
 * `omfp:chat`: the conversation of the selected thread — the transcript with the pending interactions inline at its
 * bottom (UI-070), then the queue chips and the composer.
 */

import { ComposerWidget } from "./composer.tsx";
import { Interactions } from "./interactions.tsx";
import type { WidgetProps } from "./props.ts";
import { useSelectedThread } from "./state.ts";
import { ThreadPlaceholder, Transcript } from "./transcript.tsx";

export function ChatWidget(_props: WidgetProps) {
  const { threadId, view } = useSelectedThread();
  if (threadId === undefined || view === undefined) {
    return <ThreadPlaceholder threadId={threadId} />;
  }
  return (
    <section className="omfp-chat" data-testid="thread" data-thread-id={threadId}>
      <Transcript
        key={threadId}
        threadId={threadId}
        state={view.state}
        footer={<Interactions threadId={threadId} view={view} />}
      />
      <ComposerWidget />
    </section>
  );
}
