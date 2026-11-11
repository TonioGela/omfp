/**
 * CodeMirror languages (S-13): each grammar is a dynamic `import()`, its own chunk of the bundle, loaded when a file
 * of that language opens. Other files show as plain text.
 */
import type { Extension } from "@codemirror/state";
import { languageOfPath } from "../highlight.tsx";

type Loader = () => Promise<Extension>;

/** Loaders by the highlight.js name `languageOfPath` gives (UI-090), so both highlighters agree on a file's language. */
const LOADERS: Readonly<Record<string, Loader>> = {
  typescript: async () => (await import("@codemirror/lang-javascript")).javascript({ typescript: true, jsx: true }),
  javascript: async () => (await import("@codemirror/lang-javascript")).javascript({ jsx: true }),
  rust: async () => (await import("@codemirror/lang-rust")).rust(),
  nix: async () => (await import("@replit/codemirror-lang-nix")).nix(),
  markdown: async () => (await import("@codemirror/lang-markdown")).markdown(),
  json: async () => (await import("@codemirror/lang-json")).json(),
};

/** The language extension for `path` (none for unknown languages). */
export async function languageExtension(path: string | undefined): Promise<Extension> {
  const language = path === undefined ? undefined : languageOfPath(path);
  const loader = language === undefined ? undefined : LOADERS[language];
  return loader === undefined ? [] : loader();
}
