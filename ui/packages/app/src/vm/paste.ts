/**
 * Large pastes in the composer (UI-357..UI-360): a paste above omp's threshold (more than 10 lines or 1000
 * characters, omp's `Editor`) goes into the draft as an atomic placeholder token with omp's wording,
 * `[Paste #1, +120 lines]` (or `[Paste #1, 1500 chars]` for few long lines), its text kept in the draft's `pastes`
 * and shown as a chip; sending expands every token in place.
 */

import { MAC } from "../keymap/keymap.ts";
import type { Draft, Paste } from "../sdk/keys.ts";
import type { Modifiers } from "./composer.ts";

/** A paste with more lines than this becomes a chip (omp's threshold). */
export const PASTE_MAX_LINES = 10;
/** A paste with more characters than this becomes a chip (omp's threshold). */
export const PASTE_MAX_CHARS = 1000;

/** The keys that paste raw, never as a chip (UI-359): Ctrl+Shift+V, ⌘+Shift+V on macOS (the browsers' "paste as
 * plain text"). */
export function rawPasteKeys(mac = MAC): string {
  return mac ? "Meta-Shift-v" : "Ctrl-Shift-v";
}

/** Whether a key press is {@link rawPasteKeys} (Ctrl or ⌘, either platform). */
export function isRawPaste(key: string, modifiers: Modifiers): boolean {
  return key.toLowerCase() === "v" && modifiers.shift && !modifiers.alt && (modifiers.ctrl || modifiers.meta);
}

/** Pasted text as the composer keeps it: line breaks normalized to `\n`. */
export function normalizePaste(text: string): string {
  return text.replace(/\r\n?/g, "\n");
}

/** The number of lines of `text` (a text without a line break has one). */
export function lineCount(text: string): number {
  let lines = 1;
  for (const char of text) {
    if (char === "\n") {
      lines++;
    }
  }
  return lines;
}

/** Whether pasting `text` makes a chip rather than inserting it: more than 10 lines or 1000 characters. */
export function isLargePaste(text: string): boolean {
  const normalized = normalizePaste(text);
  return normalized.length > PASTE_MAX_CHARS || lineCount(normalized) > PASTE_MAX_LINES;
}

/** The placeholder of `paste` in the draft text, as omp writes it: `[Paste #N, +L lines]` above 10 lines, else
 * `[Paste #N, C chars]`. */
export function pasteToken(paste: Paste): string {
  const lines = lineCount(paste.text);
  return lines > PASTE_MAX_LINES
    ? `[Paste #${paste.id}, +${lines} lines]`
    : `[Paste #${paste.id}, ${paste.text.length} chars]`;
}

/** Where a paste's token sits in a text. */
export interface TokenSpan {
  readonly id: number;
  readonly start: number;
  readonly end: number;
}

const TOKEN = /\[Paste #(\d+), (?:\+\d+ lines|\d+ chars)\]/g;

/** The tokens of `pastes` in `text`, in text order: only exact tokens of a known paste count, once each. */
export function tokenSpans(text: string, pastes: readonly Paste[]): TokenSpan[] {
  const spans: TokenSpan[] = [];
  const seen = new Set<number>();
  for (const match of text.matchAll(TOKEN)) {
    const id = Number(match[1]);
    const paste = pastes.find((candidate) => candidate.id === id);
    if (paste !== undefined && !seen.has(id) && match[0] === pasteToken(paste)) {
      seen.add(id);
      spans.push({ id, start: match.index, end: match.index + match[0].length });
    }
  }
  return spans;
}

/** The text to send: each paste's token replaced by its text, in one pass (a pasted text containing a token stays
 * as it is). */
export function expandPastes(text: string, pastes: readonly Paste[]): string {
  const spans = tokenSpans(text, pastes);
  if (spans.length === 0) {
    return text;
  }
  let expanded = "";
  let from = 0;
  for (const span of spans) {
    expanded += text.slice(from, span.start) + (pastes.find((paste) => paste.id === span.id)?.text ?? "");
    from = span.end;
  }
  return expanded + text.slice(from);
}

/** The draft's pastes whose token is still in its text; the others are gone. */
function keptPastes(text: string, pastes: readonly Paste[]): Paste[] {
  const kept = new Set(tokenSpans(text, pastes).map((span) => span.id));
  return pastes.filter((paste) => kept.has(paste.id));
}

/** A draft with its new text and caret. */
export interface Edited {
  readonly draft: Draft;
  /** Where the caret goes. */
  readonly caret: number;
}

/** The draft after pasting `text` as a chip over the selection `[start, end)`: the token replaces the selection (a
 * token the selection cuts goes whole, as {@link applyEdit} does), numbered after the draft's highest paste. */
export function insertPaste(draft: Draft, text: string, start: number, end: number): Edited {
  const paste: Paste = {
    id: draft.pastes.reduce((highest, existing) => Math.max(highest, existing.id), 0) + 1,
    text: normalizePaste(text),
  };
  const edited = splice(draft, start, end, pasteToken(paste));
  return { draft: { ...edited.draft, pastes: [...edited.draft.pastes, paste] }, caret: edited.caret };
}

/**
 * The draft after the text area's value became `next` (typing, deleting, cutting), with the caret at `caret` in
 * `next` when known (it tells which of equal neighbouring characters changed): tokens are atomic. A change that
 * removes part of a token removes the whole token (and its paste); text typed inside a token goes after it. The
 * caret goes after the inserted text.
 */
export function applyEdit(draft: Draft, next: string, caret = next.length): Edited {
  const previous = draft.text;
  const shortest = Math.min(previous.length, next.length);
  let prefix = 0;
  while (prefix < Math.min(shortest, caret) && previous[prefix] === next[prefix]) {
    prefix++;
  }
  let suffix = 0;
  const suffixLimit = Math.min(shortest - prefix, next.length - Math.max(caret, prefix));
  while (suffix < suffixLimit && previous[previous.length - 1 - suffix] === next[next.length - 1 - suffix]) {
    suffix++;
  }
  return splice(draft, prefix, previous.length - suffix, next.slice(prefix, next.length - suffix));
}

/** `inserted` in place of `[start, end)` of the draft's text, widened over every token it cuts; an insertion inside
 * a token goes after it. Pastes whose token went are dropped. */
function splice(draft: Draft, from: number, to: number, inserted: string): Edited {
  let start = from;
  let end = to;
  for (const span of tokenSpans(draft.text, draft.pastes)) {
    if (start === end) {
      if (span.start < start && start < span.end) {
        start = span.end;
        end = span.end;
      }
    } else if (span.start < end && start < span.end) {
      start = Math.min(start, span.start);
      end = Math.max(end, span.end);
    }
  }
  const text = draft.text.slice(0, start) + inserted + draft.text.slice(end);
  return { draft: { ...draft, text, pastes: keptPastes(text, draft.pastes) }, caret: start + inserted.length };
}

/** The draft without the paste `id`: its token and its text gone. */
export function removePaste(draft: Draft, id: number): Draft {
  const span = tokenSpans(draft.text, draft.pastes).find((candidate) => candidate.id === id);
  const text = span === undefined ? draft.text : draft.text.slice(0, span.start) + draft.text.slice(span.end);
  return { ...draft, text, pastes: draft.pastes.filter((paste) => paste.id !== id) };
}

/** The draft after the paste `id` was edited to `text`: its token follows its new size. */
export function editPaste(draft: Draft, id: number, text: string): Draft {
  const span = tokenSpans(draft.text, draft.pastes).find((candidate) => candidate.id === id);
  if (span === undefined) {
    return draft;
  }
  const paste: Paste = { id, text: normalizePaste(text) };
  return {
    ...draft,
    text: draft.text.slice(0, span.start) + pasteToken(paste) + draft.text.slice(span.end),
    pastes: draft.pastes.map((existing) => (existing.id === id ? paste : existing)),
  };
}

/** The draft with every paste expanded into its text, no chips left (a draft merged into a restored one, whose
 * paste numbers would clash). */
export function expandDraft(draft: Draft): Draft {
  return { ...draft, text: expandPastes(draft.text, draft.pastes), pastes: [] };
}
