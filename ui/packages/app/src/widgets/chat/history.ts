/**
 * The composer's prompt history (UI-361..UI-363): `Up` on the first line and `Down` on the last line walk the prompts
 * `prompts.history` gives (this thread's, then its workspace's, then the others), `Down` past the newest restoring the
 * draft typed before; typing ends the walk and keeps the recalled text as the draft.
 */

import {
  type Draft,
  type HistoryWalk,
  historyDown,
  historyUp,
  onFirstLine,
  onLastLine,
  useCall,
  useThreadSummary,
} from "omfp/sdk";
import { type KeyboardEvent, type RefObject, useRef } from "react";

/** How many prompts a walk asks for. */
const WALK_PROMPTS = 100;

/** `onKey` handles `Up` and `Down` of the field when they walk the history (it then prevents their default); `end`
 * ends a walk (call it on typing). */
export function usePromptHistory(
  threadId: string,
  draft: Draft,
  setDraft: (draft: Draft) => void,
  inputRef: RefObject<HTMLTextAreaElement | null>,
): { onKey: (event: KeyboardEvent) => boolean; end: () => void } {
  const call = useCall();
  const workspaceId = useThreadSummary(threadId)?.workspaceId;
  const walk = useRef<HistoryWalk | undefined>(undefined);
  // The prompts of a walk being started, asked once per walk.
  const asking = useRef(false);
  const show = (next: Draft) => {
    const input = inputRef.current;
    if (input !== null) {
      input.value = next.text;
      input.setSelectionRange(next.text.length, next.text.length);
    }
    setDraft(next);
  };
  return {
    onKey: (event) => {
      const input = inputRef.current;
      if (input === null || event.shiftKey || event.altKey || event.ctrlKey || event.metaKey) {
        return false;
      }
      const { selectionStart, selectionEnd, value } = input;
      if (event.key === "ArrowUp" && onFirstLine(value, selectionStart, selectionEnd)) {
        event.preventDefault();
        if (walk.current !== undefined) {
          const step = historyUp(walk.current, walk.current.prompts, draft);
          if (step !== undefined) {
            walk.current = step.walk;
            show(step.draft);
          }
        } else if (!asking.current) {
          asking.current = true;
          const saved = draft;
          call("prompts.history", {
            threadId,
            ...(workspaceId === undefined ? {} : { workspaceId }),
            limit: WALK_PROMPTS,
          }).then(
            (history) => {
              asking.current = false;
              const step = historyUp(
                undefined,
                history.prompts.map((prompt) => prompt.text),
                saved,
              );
              if (step !== undefined) {
                walk.current = step.walk;
                show(step.draft);
              }
            },
            () => {
              asking.current = false;
            },
          );
        }
        return true;
      }
      if (event.key === "ArrowDown" && walk.current !== undefined && onLastLine(value, selectionStart, selectionEnd)) {
        event.preventDefault();
        const step = historyDown(walk.current);
        if (step !== undefined) {
          walk.current = step.walk;
          show(step.draft);
        }
        return true;
      }
      return false;
    },
    end: () => {
      walk.current = undefined;
    },
  };
}
