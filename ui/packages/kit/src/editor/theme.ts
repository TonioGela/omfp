/**
 * The CodeMirror theme from omfp tokens only (UI-091, UI-125, S-13): every color is a `var(--omfp-*)` or a `color-mix()`
 * of them, so a token change (light/dark, a user or matugen theme) recolors a mounted editor or merge view without
 * reconfiguring it. Crush (09): code family, focus ring of UI-132, selection primary at 28 % (UI-138), gutters and
 * panels on the code and popover surfaces, diff colors of UI-138.
 */
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

const v = (token: string) => `var(--omfp-${token})`;
const mix = (token: string, percent: number) => `color-mix(in oklab, var(--omfp-${token}) ${percent}%, transparent)`;

export const omfpTheme = EditorView.theme({
  "&": {
    color: v("on-surface"),
    backgroundColor: v("code-bg"),
    fontSize: v("fs-code"),
    height: "100%",
  },
  "&.cm-focused": {
    outline: `${v("focus-w")} solid ${v("focus-color")}`,
    outlineOffset: `calc(-1 * ${v("focus-w")})`,
  },
  ".cm-scroller": { fontFamily: v("font-code"), lineHeight: v("lh-code") },
  ".cm-content": { caretColor: v("primary") },
  ".cm-cursor, .cm-dropCursor": { borderLeft: `2px solid ${v("primary")}` },
  // vim's block cursor in normal mode; through `.cm-scroller` to outrank the vim extension's own (highest-precedence)
  // colors.
  "& .cm-scroller .cm-fat-cursor": { background: mix("primary", 70), color: v("on-primary") },
  "&:not(.cm-focused) .cm-scroller .cm-fat-cursor": { background: "none", outline: `1px solid ${v("primary")}` },
  "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, ::selection": {
    backgroundColor: mix("primary", 28),
  },
  "&:not(.cm-focused) .cm-selectionBackground": { backgroundColor: mix("primary", 16) },
  ".cm-activeLine": { backgroundColor: mix("primary", 5) },
  ".cm-gutters": {
    backgroundColor: v("code-bg"),
    color: mix("on-surface-variant", 70),
    borderRight: `1px solid ${v("rule")}`,
  },
  ".cm-gutterElement": { padding: `0 ${v("s2")} 0 ${v("s3")}` },
  ".cm-activeLineGutter": { backgroundColor: mix("primary", 8), color: v("primary") },
  ".cm-foldPlaceholder": {
    backgroundColor: mix("tertiary", 10),
    border: `1px solid ${mix("tertiary", 40)}`,
    color: v("tertiary"),
    borderRadius: v("r-chip"),
  },
  ".cm-matchingBracket, &.cm-focused .cm-matchingBracket": {
    backgroundColor: mix("tertiary", 22),
    outline: `1px solid ${mix("tertiary", 50)}`,
  },
  ".cm-nonmatchingBracket, &.cm-focused .cm-nonmatchingBracket": { backgroundColor: mix("error", 22) },
  ".cm-searchMatch": { backgroundColor: mix("warn", 28), outline: `1px solid ${mix("warn", 55)}` },
  ".cm-searchMatch-selected, .cm-searchMatch.cm-searchMatch-selected": { backgroundColor: mix("warn", 55) },
  ".cm-selectionMatch": { backgroundColor: mix("primary", 14) },
  // Search, go-to-line and vim's command panel: the popover surface under a rule, code family.
  ".cm-panels": { backgroundColor: v("pop-bg"), color: v("on-surface"), fontFamily: v("font-code") },
  ".cm-panels-top": { borderBottom: `1px solid ${v("rule")}` },
  ".cm-panels-bottom": { borderTop: `1px solid ${v("rule")}` },
  ".cm-panel": { fontSize: v("fs-xs"), padding: `${v("s1")} ${v("s2")}` },
  ".cm-panel input, .cm-panel button, .cm-panel label": { fontFamily: v("font-code"), fontSize: v("fs-xs") },
  ".cm-textfield": {
    color: v("on-surface"),
    backgroundColor: "transparent",
    border: `1px solid ${v("rule-strong")}`,
    borderRadius: v("r-ctl"),
  },
  ".cm-textfield:focus": { outline: `${v("focus-w")} solid ${v("focus-color")}`, borderColor: v("primary") },
  ".cm-button": {
    color: v("on-surface"),
    backgroundImage: "none",
    backgroundColor: "transparent",
    border: `1px solid ${v("rule-strong")}`,
    borderRadius: v("r-ctl"),
  },
  ".cm-button:hover": { backgroundColor: v("hover"), borderColor: v("outline") },
  ".cm-panel.cm-search [name=close]": { color: v("on-surface-variant") },
  ".cm-vim-panel": { padding: `${v("s1")} ${v("s2")}` },
  ".cm-vim-panel input": { color: v("on-surface"), fontFamily: v("font-code"), caretColor: v("tertiary") },
  ".cm-tooltip": {
    backgroundColor: v("pop-bg"),
    color: v("on-surface"),
    border: `1px solid ${v("pop-border")}`,
    borderRadius: v("r-ctl"),
    boxShadow: v("pop-shadow"),
  },
  // @codemirror/merge: unified (deleted chunks as widgets, the editor is the new text) and side by side (a = old,
  // b = new); UI-138's added and removed colors.
  "&.cm-merge-a .cm-changedLine, .cm-deletedChunk": { backgroundColor: mix("error", 11) },
  "&.cm-merge-b .cm-changedLine, .cm-inlineChangedLine": { backgroundColor: mix("ok", 12) },
  // Removed text: error 70 % into on-surface (UI-138).
  ".cm-deletedChunk": { color: "color-mix(in oklab, var(--omfp-error) 70%, var(--omfp-on-surface))" },
  "&.cm-merge-a .cm-changedText, .cm-deletedChunk .cm-deletedText": { background: mix("error", 28) },
  "&.cm-merge-b .cm-changedText": { background: mix("ok", 30) },
  "&.cm-merge-b .cm-deletedText": { background: mix("error", 28) },
  "&.cm-merge-a .cm-changedLineGutter, .cm-deletedLineGutter": { background: v("error") },
  "&.cm-merge-b .cm-changedLineGutter": { background: v("ok") },
  ".cm-inlineChangedLineGutter": { background: v("tertiary") },
  ".cm-collapsedLines": {
    color: v("on-surface-variant"),
    background: v("surface-container"),
    fontSize: v("fs-2xs"),
  },
  ".cm-merge-revert": { background: v("code-bg") },
  ".cm-merge-revert button": { color: v("primary"), cursor: "pointer" },
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
    { tag: [t.string, t.special(t.string), t.regexp, t.inserted], color: v("ok") },
    { tag: [t.number, t.bool, t.null, t.atom, t.constant(t.name), t.meta, t.url], color: v("warn") },
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
