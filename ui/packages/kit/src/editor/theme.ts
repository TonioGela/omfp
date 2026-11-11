/**
 * The CodeMirror theme from omfp tokens only (UI-091, S-13): every color is a `var(--omfp-*)`, so a token change
 * (light/dark, a user or matugen theme) recolors a mounted editor or merge view without reconfiguring it.
 */
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

const v = (token: string) => `var(--omfp-${token})`;
const mix = (token: string, percent: number) => `color-mix(in srgb, var(--omfp-${token}) ${percent}%, transparent)`;

export const omfpTheme = EditorView.theme({
  "&": {
    color: v("on-surface"),
    backgroundColor: v("surface"),
    fontSize: v("font-size-sm"),
    height: "100%",
  },
  "&.cm-focused": { outline: `2px solid ${v("primary")}`, outlineOffset: "-2px" },
  ".cm-scroller": { fontFamily: v("font-code"), lineHeight: v("line-height") },
  ".cm-content": { caretColor: v("primary") },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: v("primary") },
  // vim's block cursor in normal mode.
  ".cm-fat-cursor": { background: mix("primary", 60), color: v("on-primary") },
  "&:not(.cm-focused) .cm-fat-cursor": { background: "none", outline: `1px solid ${v("primary")}` },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: mix("primary", 25),
  },
  ".cm-activeLine": { backgroundColor: mix("on-surface", 5) },
  ".cm-gutters": {
    backgroundColor: v("surface-container-low"),
    color: v("on-surface-variant"),
    borderRight: `1px solid ${v("outline-variant")}`,
  },
  ".cm-activeLineGutter": { backgroundColor: v("surface-container-high"), color: v("on-surface") },
  ".cm-matchingBracket": { backgroundColor: mix("tertiary", 25), outline: "none" },
  ".cm-searchMatch": { backgroundColor: mix("warning", 30) },
  ".cm-searchMatch-selected": { backgroundColor: mix("warning", 55) },
  ".cm-panels": { backgroundColor: v("surface-container"), color: v("on-surface") },
  ".cm-panels-bottom": { borderTop: `1px solid ${v("outline-variant")}` },
  ".cm-vim-panel input": { color: v("on-surface"), fontFamily: v("font-code") },
  // @codemirror/merge: unified (deleted chunks as widgets, the editor is the new text) and side by side (a = old,
  // b = new).
  "&.cm-merge-a .cm-changedLine, .cm-deletedChunk": { backgroundColor: mix("error", 12) },
  "&.cm-merge-b .cm-changedLine, .cm-inlineChangedLine": { backgroundColor: mix("success", 14) },
  "&.cm-merge-a .cm-changedText, .cm-deletedChunk .cm-deletedText": { background: mix("error", 30) },
  "&.cm-merge-b .cm-changedText": { background: mix("success", 32) },
  "&.cm-merge-b .cm-deletedText": { background: mix("error", 30) },
  "&.cm-merge-a .cm-changedLineGutter, .cm-deletedLineGutter": { background: v("error") },
  "&.cm-merge-b .cm-changedLineGutter": { background: v("success") },
  ".cm-inlineChangedLineGutter": { background: v("tertiary") },
  ".cm-collapsedLines": { color: v("on-surface-variant"), background: v("surface-container") },
  ".cm-merge-revert": { background: v("surface-container-low") },
});

/** The same role mapping as kit.css's `hljs-*` colors (UI-090), so CodeMirror and lowlight code look alike. */
export const omfpHighlight = syntaxHighlighting(
  HighlightStyle.define([
    {
      tag: [t.keyword, t.modifier, t.controlKeyword, t.operatorKeyword, t.definitionKeyword, t.moduleKeyword],
      color: v("primary"),
    },
    { tag: [t.typeName, t.className, t.namespace, t.standard(t.name)], color: v("primary") },
    {
      tag: [t.function(t.variableName), t.function(t.propertyName), t.heading, t.labelName, t.tagName],
      color: v("tertiary"),
    },
    { tag: [t.string, t.special(t.string), t.regexp, t.inserted], color: v("success") },
    { tag: [t.number, t.bool, t.null, t.atom, t.constant(t.name), t.meta, t.url], color: v("warning") },
    { tag: [t.propertyName, t.attributeName, t.variableName, t.definition(t.variableName)], color: v("secondary") },
    { tag: [t.comment, t.quote], color: v("on-surface-variant"), fontStyle: "italic" },
    { tag: [t.punctuation, t.operator, t.bracket], color: v("outline") },
    { tag: [t.deleted, t.invalid], color: v("error") },
    { tag: t.emphasis, fontStyle: "italic" },
    { tag: t.strong, fontWeight: "bold" },
    { tag: t.link, textDecoration: "underline" },
    { tag: t.heading, fontWeight: "bold" },
  ]),
);
