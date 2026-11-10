/**
 * Pending interactions (UI-070..073): titles, countdowns, the `ask` form with its selection rules, and requests that
 * another client or omp closed.
 */

import type { ClosedInteraction, InteractionRequest } from "@omfp/client";
import { type AskAnswer, type AskUiRequest, validateAskAnswers } from "@omfp/core";
import { strings } from "../strings/index.ts";

/** The dialog requests: they wait for an answer. */
export type Dialog = Extract<InteractionRequest, { method: "select" | "confirm" | "input" | "editor" | "ask" }>;

const DIALOGS: Readonly<Record<string, true>> = { select: true, confirm: true, input: true, editor: true, ask: true };

export function isDialog(request: InteractionRequest): request is Dialog {
  return DIALOGS[request.method] === true;
}

/**
 * The key of a request's card: its id, so closing an earlier request never remounts the cards after it (typed text and
 * focus stay); the position only for requests that are not dialogs.
 */
export function cardKey(request: InteractionRequest, index: number): string {
  return isDialog(request) ? `id-${request.id}` : `at-${index}`;
}

/** The request's title (the first question for `ask`). */
export function title(request: InteractionRequest): string {
  if (request.method === "ask") {
    const first = request.questions[0];
    return first === undefined ? "" : (first.header ?? first.question);
  }
  return isDialog(request) ? request.title : "";
}

/** Timeout announced by the request, in milliseconds (`editor` has none). */
export function timeoutMs(request: InteractionRequest): number | undefined {
  switch (request.method) {
    case "select":
    case "confirm":
    case "input":
    case "ask":
      return request.timeout !== undefined && request.timeout > 0 ? request.timeout : undefined;
    default:
      return undefined;
  }
}

/** Milliseconds left of `timeout` for a request first shown at `shownAt`, never negative (UI-073). */
export function remainingMs(timeout: number, shownAt: number, now: number): number {
  return Math.max(0, timeout - Math.max(0, now - shownAt));
}

/** `m:ss`, rounding up so the countdown shows `0:00` only when time is up. */
export function formatCountdown(remaining: number): string {
  const seconds = Math.ceil(Math.max(0, remaining) / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/** The answer being built for one `ask` question. */
export interface AnswerDraft {
  readonly selected: readonly string[];
  /** Free text; offered for every question since omp never lists "Other" (UI-071). */
  readonly custom: string;
}

/** The `ask` form: one draft per question, in order. */
export type AskForm = readonly AnswerDraft[];

export function emptyAskForm(request: AskUiRequest): AskForm {
  return request.questions.map(() => ({ selected: [], custom: "" }));
}

function withAnswer(form: AskForm, index: number, answer: AnswerDraft): AskForm {
  return form.map((draft, position) => (position === index ? answer : draft));
}

/**
 * Picks `label` for question `index`: a single-select question takes it instead of any other choice or free text; a
 * multi-select one toggles it.
 */
export function toggleOption(form: AskForm, request: AskUiRequest, index: number, label: string): AskForm {
  const question = request.questions[index];
  const draft = form[index];
  if (question === undefined || draft === undefined) {
    return form;
  }
  if (!question.multi) {
    return withAnswer(form, index, { selected: [label], custom: "" });
  }
  const selected = draft.selected.includes(label)
    ? draft.selected.filter((chosen) => chosen !== label)
    : [...draft.selected, label];
  return withAnswer(form, index, { ...draft, selected });
}

/** Sets the free text of question `index`; on a single-select question non-blank text replaces the choice. */
export function setCustom(form: AskForm, request: AskUiRequest, index: number, text: string): AskForm {
  const question = request.questions[index];
  const draft = form[index];
  if (question === undefined || draft === undefined) {
    return form;
  }
  const selected = !question.multi && text.trim() !== "" ? [] : draft.selected;
  return withAnswer(form, index, { selected, custom: text });
}

/** The answers in question order, selected options in the request's option order. */
export function askAnswers(form: AskForm, request: AskUiRequest): AskAnswer[] {
  return request.questions.map((question, index) => {
    const draft = form[index] ?? { selected: [], custom: "" };
    const custom = draft.custom.trim();
    const selectedOptions = question.options
      .filter((option) => draft.selected.includes(option.label))
      .map((option) => option.label);
    return custom === ""
      ? { id: question.id, selectedOptions }
      : { id: question.id, selectedOptions, customInput: custom };
  });
}

/** The answers when every question has one and omp's rules hold (DMN-054), else what is wrong. */
export type AskCheck =
  | { readonly ok: true; readonly answers: AskAnswer[] }
  | { readonly ok: false; readonly error: string };

export function checkAsk(form: AskForm, request: AskUiRequest): AskCheck {
  const answers = askAnswers(form, request);
  const missing = request.questions.find((_, index) => {
    const answer = answers[index];
    return answer !== undefined && answer.selectedOptions.length === 0 && answer.customInput === undefined;
  });
  if (missing !== undefined) {
    return { ok: false, error: strings.chat.interactions.answerRequired(missing.question) };
  }
  const error = validateAskAnswers(request, answers);
  return error === undefined ? { ok: true, answers } : { ok: false, error };
}

/** A closed request shown in place of the dialog (UI-072). */
export interface ClosedRow {
  readonly id: string;
  readonly title: string;
  readonly text: string;
}

/**
 * Rows for requests closed by someone else: another client (with its name) or omp. Requests answered from this window
 * (`answeredHere`) or dismissed need no row.
 */
export function closedRows(
  closed: readonly ClosedInteraction[],
  answeredHere: (id: string) => boolean,
  dismissed: (id: string) => boolean,
): ClosedRow[] {
  return closed.flatMap(({ request, resolution }) => {
    if (!isDialog(request) || answeredHere(request.id) || dismissed(request.id)) {
      return [];
    }
    const text =
      resolution === undefined
        ? strings.chat.interactions.closedByOmp
        : strings.chat.interactions.answeredElsewhere(resolution.client);
    return [{ id: request.id, title: title(request), text }];
  });
}
