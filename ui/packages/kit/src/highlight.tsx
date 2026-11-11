/**
 * Code highlighting (UI-090, spike S-03): highlight.js grammars through lowlight, each loaded on first use with a
 * dynamic `import()` (its own chunk of the bundle). The result is `hljs-*` class names that kit.css colors with the
 * theme's role tokens, so light, dark and user themes apply without rebuilding anything.
 */

import type { LanguageFn } from "highlight.js";
import { createLowlight } from "lowlight";
import { Fragment, type ReactNode, useEffect, useMemo, useState } from "react";

type Loader = () => Promise<{ default: LanguageFn }>;

/** The grammars that can be loaded, by highlight.js name. */
const GRAMMARS: Readonly<Record<string, Loader>> = {
  bash: () => import("highlight.js/lib/languages/bash"),
  c: () => import("highlight.js/lib/languages/c"),
  cpp: () => import("highlight.js/lib/languages/cpp"),
  csharp: () => import("highlight.js/lib/languages/csharp"),
  css: () => import("highlight.js/lib/languages/css"),
  diff: () => import("highlight.js/lib/languages/diff"),
  dockerfile: () => import("highlight.js/lib/languages/dockerfile"),
  go: () => import("highlight.js/lib/languages/go"),
  graphql: () => import("highlight.js/lib/languages/graphql"),
  ini: () => import("highlight.js/lib/languages/ini"),
  java: () => import("highlight.js/lib/languages/java"),
  javascript: () => import("highlight.js/lib/languages/javascript"),
  json: () => import("highlight.js/lib/languages/json"),
  kotlin: () => import("highlight.js/lib/languages/kotlin"),
  lua: () => import("highlight.js/lib/languages/lua"),
  makefile: () => import("highlight.js/lib/languages/makefile"),
  markdown: () => import("highlight.js/lib/languages/markdown"),
  nix: () => import("highlight.js/lib/languages/nix"),
  php: () => import("highlight.js/lib/languages/php"),
  python: () => import("highlight.js/lib/languages/python"),
  ruby: () => import("highlight.js/lib/languages/ruby"),
  rust: () => import("highlight.js/lib/languages/rust"),
  scss: () => import("highlight.js/lib/languages/scss"),
  shell: () => import("highlight.js/lib/languages/shell"),
  sql: () => import("highlight.js/lib/languages/sql"),
  swift: () => import("highlight.js/lib/languages/swift"),
  typescript: () => import("highlight.js/lib/languages/typescript"),
  xml: () => import("highlight.js/lib/languages/xml"),
  yaml: () => import("highlight.js/lib/languages/yaml"),
};

/** Other names (Markdown fence info, file extensions) of the grammars. */
const ALIASES: Readonly<Record<string, string>> = {
  cc: "cpp",
  cjs: "javascript",
  console: "shell",
  cs: "csharp",
  cxx: "cpp",
  golang: "go",
  gql: "graphql",
  h: "c",
  hpp: "cpp",
  htm: "xml",
  html: "xml",
  js: "javascript",
  jsonc: "json",
  jsx: "javascript",
  kt: "kotlin",
  kts: "kotlin",
  md: "markdown",
  mjs: "javascript",
  mk: "makefile",
  mts: "typescript",
  patch: "diff",
  py: "python",
  rb: "ruby",
  rs: "rust",
  sh: "bash",
  svg: "xml",
  toml: "ini",
  ts: "typescript",
  tsx: "typescript",
  yml: "yaml",
  zsh: "bash",
};

/** Files known by their whole name. */
const FILE_NAMES: Readonly<Record<string, string>> = {
  Dockerfile: "dockerfile",
  GNUmakefile: "makefile",
  Makefile: "makefile",
  "flake.lock": "json",
};

/** The grammar of a language name or alias (`tsx`, `rs`, `Rust`), if there is one. */
export function grammarOf(language: string): string | undefined {
  const name = language.trim().toLowerCase();
  if (Object.hasOwn(GRAMMARS, name)) {
    return name;
  }
  return Object.hasOwn(ALIASES, name) ? ALIASES[name] : undefined;
}

/** The grammar of a file, from its name or extension. */
export function languageOfPath(path: string): string | undefined {
  const name = path.split(/[\\/]/).pop() ?? path;
  if (Object.hasOwn(FILE_NAMES, name)) {
    return FILE_NAMES[name];
  }
  const dot = name.lastIndexOf(".");
  return dot <= 0 ? undefined : grammarOf(name.slice(dot + 1));
}

const lowlight = createLowlight();
const loading = new Map<string, Promise<void>>();

/** Loads `language`'s grammar (once); resolves when it can highlight, rejects for an unknown language. */
export function loadLanguage(language: string): Promise<void> {
  const grammar = grammarOf(language);
  if (grammar === undefined) {
    return Promise.reject(new Error(`no grammar for ${language}`));
  }
  const pending = loading.get(grammar);
  if (pending !== undefined) {
    return pending;
  }
  const loader = GRAMMARS[grammar];
  const load = (loader === undefined ? Promise.reject(new Error(grammar)) : loader()).then((module) => {
    if (!lowlight.registered(grammar)) {
      lowlight.register(grammar, module.default);
    }
  });
  loading.set(grammar, load);
  return load;
}

/** A run of text with the `hljs-*` classes of the tokens around it. */
export interface Token {
  readonly text: string;
  readonly className: string | undefined;
}

/** Highlighted code, one token list per line. */
export type HighlightedLines = readonly (readonly Token[])[];

/** The nodes lowlight returns: elements with class names, and text. */
type Node =
  | { readonly type: "text"; readonly value: string }
  | {
      readonly type: "element";
      readonly properties?: { readonly className?: unknown };
      readonly children: readonly Node[];
    }
  | { readonly type: string };

function addTokens(nodes: readonly Node[], className: string | undefined, lines: Token[][]): void {
  for (const node of nodes) {
    if (node.type === "text" && "value" in node) {
      node.value.split("\n").forEach((part, index) => {
        if (index > 0) {
          lines.push([]);
        }
        if (part !== "") {
          lines[lines.length - 1]?.push({ text: part, className });
        }
      });
    } else if (node.type === "element" && "children" in node) {
      const own = node.properties?.className;
      const names = Array.isArray(own) ? own.filter((name) => typeof name === "string").join(" ") : "";
      const nested = [className, names].filter((name) => name !== undefined && name !== "").join(" ");
      addTokens(node.children, nested === "" ? undefined : nested, lines);
    }
  }
}

/** `code` highlighted as `language`, when its grammar is loaded (see {@link loadLanguage}); else `undefined`. */
export function highlightLines(code: string, language: string): HighlightedLines | undefined {
  const grammar = grammarOf(language);
  if (grammar === undefined || !lowlight.registered(grammar)) {
    return undefined;
  }
  const lines: Token[][] = [[]];
  addTokens(lowlight.highlight(grammar, code).children as readonly Node[], undefined, lines);
  return lines;
}

/**
 * `code` highlighted as `language`: `undefined` while the grammar loads (render the plain text meanwhile), and for no
 * or an unknown language.
 */
export function useHighlight(code: string, language: string | undefined): HighlightedLines | undefined {
  const grammar = language === undefined ? undefined : grammarOf(language);
  const [loaded, setLoaded] = useState(() => grammar !== undefined && lowlight.registered(grammar));
  useEffect(() => {
    if (grammar === undefined || lowlight.registered(grammar)) {
      setLoaded(grammar !== undefined);
      return;
    }
    let live = true;
    setLoaded(false);
    loadLanguage(grammar).then(
      () => live && setLoaded(true),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [grammar]);
  return useMemo(
    () => (loaded && grammar !== undefined ? highlightLines(code, grammar) : undefined),
    [code, grammar, loaded],
  );
}

/** One highlighted line. */
export function TokenLine({ tokens }: { tokens: readonly Token[] }): ReactNode {
  return tokens.map((token, index) =>
    token.className === undefined ? (
      // biome-ignore lint/suspicious/noArrayIndexKey: tokens of one highlight never move
      <Fragment key={index}>{token.text}</Fragment>
    ) : (
      // biome-ignore lint/suspicious/noArrayIndexKey: tokens of one highlight never move
      <span key={index} className={token.className}>
        {token.text}
      </span>
    ),
  );
}

/** `code` with the highlighting of `language` once its grammar is loaded, plain text before. */
export function Highlighted({ code, language }: { code: string; language: string | undefined }): ReactNode {
  const lines = useHighlight(code, language);
  if (lines === undefined) {
    return code;
  }
  return lines.map((tokens, index) => (
    // biome-ignore lint/suspicious/noArrayIndexKey: lines of one highlight never move
    <Fragment key={index}>
      {index > 0 && "\n"}
      <TokenLine tokens={tokens} />
    </Fragment>
  ));
}
