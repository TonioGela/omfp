/**
 * Validation of `ask` dialog answers (port of `omfp-core/src/ask.rs`; DMN-054), mirroring omp's own check (rpc.md,
 * "Inbound response"): any other shape fails omp's `ask` call, so the daemon rejects it before forwarding.
 */

import type { AskAnswer, AskUiRequest } from "./wire.ts";

/** A string quoted the way Rust's `{:?}` shows it. */
const quoted = JSON.stringify;

/**
 * Checks `answers` against `request`: one answer per question in request order with the question's id; selected
 * options are exact, distinct labels of that question; a single-select question takes at most one option and not both
 * an option and non-blank custom input. Returns a description of the first violation, `undefined` when valid.
 */
export function validateAskAnswers(request: AskUiRequest, answers: readonly AskAnswer[]): string | undefined {
  if (answers.length !== request.questions.length) {
    return `expected ${request.questions.length} answers in question order, got ${answers.length}`;
  }
  for (const [index, question] of request.questions.entries()) {
    const answer = answers[index];
    if (answer === undefined) {
      continue;
    }
    if (answer.id !== question.id) {
      return `answer ${index} must have id ${quoted(question.id)}, got ${quoted(answer.id)}`;
    }
    for (const [position, label] of answer.selectedOptions.entries()) {
      if (!question.options.some((option) => option.label === label)) {
        return `answer ${quoted(question.id)} selected unknown option ${quoted(label)}`;
      }
      if (answer.selectedOptions.slice(0, position).includes(label)) {
        return `answer ${quoted(question.id)} selected ${quoted(label)} twice`;
      }
    }
    const custom = answer.customInput !== undefined && answer.customInput.trim() !== "";
    const selected = answer.selectedOptions.length;
    if (!question.multi && (selected > 1 || (selected === 1 && custom))) {
      return `answer ${quoted(question.id)} is single-select but carries more than one answer`;
    }
  }
  return undefined;
}
