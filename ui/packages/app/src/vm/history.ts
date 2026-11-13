/**
 * Prompt history in the composer (UI-361..UI-363), like omp's TUI: `Up` with the caret on the first line recalls the
 * previous prompt, `Down` on the last line goes forward and, past the newest, restores the draft typed before. The
 * prompts come from `prompts.history` (PRT-127): this thread's first, then its workspace's, then every other, newest
 * first within each.
 */

import type { Draft } from "../sdk/keys.ts";
import { EMPTY_DRAFT } from "../sdk/keys.ts";

/** A history walk in progress: the prompts, the one shown (`0` the newest) and the draft to come back to. */
export interface HistoryWalk {
  readonly prompts: readonly string[];
  readonly index: number;
  readonly saved: Draft;
}

/** Whether `Up` may recall: no selection and no line break before the caret. */
export function onFirstLine(text: string, start: number, end: number): boolean {
  return start === end && !text.slice(0, start).includes("\n");
}

/** Whether `Down` may go forward: no selection and no line break after the caret. */
export function onLastLine(text: string, start: number, end: number): boolean {
  return start === end && !text.slice(end).includes("\n");
}

/** A step of the walk: the walk after it (`undefined` once it ended) and the draft to show. */
export interface HistoryStep {
  readonly walk: HistoryWalk | undefined;
  readonly draft: Draft;
}

/**
 * `Up`: the next older prompt. Starting a walk (`walk` undefined) saves `draft` and shows the newest of `prompts`;
 * `undefined` when there is nothing older (the key then does nothing).
 */
export function historyUp(
  walk: HistoryWalk | undefined,
  prompts: readonly string[],
  draft: Draft,
): HistoryStep | undefined {
  const current = walk ?? { prompts, index: -1, saved: draft };
  const index = current.index + 1;
  const text = current.prompts[index];
  if (text === undefined) {
    return undefined;
  }
  return { walk: { ...current, index }, draft: { ...EMPTY_DRAFT, text } };
}

/** `Down`: the next newer prompt, and past the newest the saved draft, which ends the walk; `undefined` outside a
 * walk. */
export function historyDown(walk: HistoryWalk | undefined): HistoryStep | undefined {
  if (walk === undefined) {
    return undefined;
  }
  const index = walk.index - 1;
  const text = walk.prompts[index];
  if (index < 0 || text === undefined) {
    return { walk: undefined, draft: walk.saved };
  }
  return { walk: { ...walk, index }, draft: { ...EMPTY_DRAFT, text } };
}
