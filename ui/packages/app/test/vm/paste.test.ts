/** L0: large pastes as chips (UI-357..UI-359): threshold, tokens, expansion, atomic edits. */

import { describe, expect, test } from "bun:test";
import type { Draft } from "../../src/sdk/index.ts";
import {
  applyEdit,
  editPaste,
  expandPastes,
  insertPaste,
  isLargePaste,
  isRawPaste,
  pasteToken,
  rawPasteKeys,
  removePaste,
  tokenSpans,
} from "../../src/vm/paste.ts";

const lines = (count: number) => Array.from({ length: count }, (_, index) => `line ${index + 1}`).join("\n");

function draft(text: string, pastes: Draft["pastes"] = []): Draft {
  return { text, images: [], pastes };
}

const NONE = { shift: false, alt: false, ctrl: false, meta: false };

describe("paste threshold and tokens", () => {
  // spec: UI-357
  test("more than 10 lines or 1000 characters is large, as in omp; CRLF counts as one break", () => {
    expect(isLargePaste(lines(10))).toBe(false);
    expect(isLargePaste(lines(11))).toBe(true);
    expect(isLargePaste(lines(10).replaceAll("\n", "\r\n"))).toBe(false);
    expect(isLargePaste("x".repeat(1000))).toBe(false);
    expect(isLargePaste("x".repeat(1001))).toBe(true);
  });

  // spec: UI-357
  test("tokens use omp's wording: lines above 10 lines, else characters", () => {
    expect(pasteToken({ id: 1, text: lines(120) })).toBe("[Paste #1, +120 lines]");
    expect(pasteToken({ id: 2, text: "x".repeat(1500) })).toBe("[Paste #2, 1500 chars]");
  });

  // spec: UI-357
  test("a paste replaces the selection with the next numbered token and keeps its text, line breaks normalized", () => {
    const first = insertPaste(draft("ab cd"), lines(12).replaceAll("\n", "\r\n"), 2, 3);
    expect(first.draft.text).toBe("ab[Paste #1, +12 lines]cd");
    expect(first.caret).toBe("ab[Paste #1, +12 lines]".length);
    expect(first.draft.pastes).toEqual([{ id: 1, text: lines(12) }]);
    const second = insertPaste(first.draft, "y".repeat(1001), 0, 0);
    expect(second.draft.text).toBe("[Paste #2, 1001 chars]ab[Paste #1, +12 lines]cd");
    expect(second.draft.pastes.map((paste) => paste.id)).toEqual([1, 2]);
  });

  // spec: UI-357
  test("sending expands each known token in place, once, never rescanning pasted text", () => {
    const pastes = [
      { id: 1, text: lines(11) },
      { id: 2, text: "inner [Paste #1, +11 lines] stays" },
    ];
    const text = "a [Paste #1, +11 lines] b [Paste #2, 33 chars] c [Paste #3, +12 lines]";
    expect(expandPastes(text, pastes)).toBe(
      `a ${lines(11)} b inner [Paste #1, +11 lines] stays c [Paste #3, +12 lines]`,
    );
    // A token whose size no longer matches its paste is plain text.
    expect(tokenSpans("[Paste #1, +12 lines]", pastes)).toEqual([]);
  });
});

describe("atomic tokens", () => {
  const paste = { id: 1, text: lines(11) };
  const token = "[Paste #1, +11 lines]";

  // spec: UI-358
  test("deleting one character of a token removes the whole token and its paste", () => {
    const before = draft(`a ${token} b`, [paste]);
    // Backspace after the token's last `]`.
    const backspace = applyEdit(before, `a ${token.slice(0, -1)} b`, 2 + token.length - 1);
    expect(backspace.draft).toEqual(draft("a  b"));
    expect(backspace.caret).toBe(2);
    // Delete at the token's start.
    expect(applyEdit(before, `a ${token.slice(1)} b`, 2).draft).toEqual(draft("a  b"));
    // A selection over part of the token and the text before it.
    expect(applyEdit(before, `X${token.slice(4)} b`, 1).draft).toEqual(draft("X b"));
  });

  // spec: UI-358
  test("a deleted `]` right after a token is told apart from the token's own by the caret", () => {
    const before = draft(`${token}]`, [paste]);
    expect(applyEdit(before, token, token.length).draft).toEqual(draft(token, [paste]));
    expect(applyEdit(before, token, token.length - 1).draft).toEqual(draft("]"));
  });

  // spec: UI-358
  test("text typed inside a token goes after it; typing elsewhere keeps the paste", () => {
    const before = draft(`a ${token}`, [paste]);
    const inside = applyEdit(before, `a [Paste #1,x +11 lines]`, 13);
    expect(inside.draft).toEqual(draft(`a ${token}x`, [paste]));
    expect(inside.caret).toBe(2 + token.length + 1);
    expect(applyEdit(before, `ab ${token}`, 2).draft).toEqual(draft(`ab ${token}`, [paste]));
  });

  // spec: UI-358
  test("× removes a paste with its token; editing its text resizes its token", () => {
    const before = draft(`a ${token} b`, [paste]);
    expect(removePaste(before, 1)).toEqual(draft("a  b"));
    expect(editPaste(before, 1, lines(3))).toEqual(draft("a [Paste #1, 20 chars] b", [{ id: 1, text: lines(3) }]));
  });
});

describe("raw paste", () => {
  // spec: UI-359
  test("Ctrl+Shift+V or ⌘+Shift+V pastes raw; the label follows the platform", () => {
    expect(isRawPaste("V", { ...NONE, ctrl: true, shift: true })).toBe(true);
    expect(isRawPaste("v", { ...NONE, meta: true, shift: true })).toBe(true);
    expect(isRawPaste("v", { ...NONE, ctrl: true })).toBe(false);
    expect(isRawPaste("v", { ...NONE, ctrl: true, shift: true, alt: true })).toBe(false);
    expect([rawPasteKeys(false), rawPasteKeys(true)]).toEqual(["Ctrl-Shift-v", "Meta-Shift-v"]);
  });
});
