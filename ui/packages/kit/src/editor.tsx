/**
 * CodeMirror 6 in the kit (S-13, UI-110..114): a file editor with vim bindings and a merge view for diffs. Nothing of
 * CodeMirror is in the kit's own chunk: the first `CodeEditor` or `CodeDiff` imports the editor chunk, diffs then the
 * merge chunk, and each language its grammar (`editor/languages.ts`), so windows that never open a file never load
 * them. Colors come from the theme tokens only (`editor/theme.ts`).
 */
import { type HTMLAttributes, lazy, type ReactNode, type RefObject, Suspense } from "react";
import { cx } from "./class.ts";

/** vim's mode while the editor has the focus (`vim-mode-change`). */
export type VimMode = "normal" | "insert" | "visual" | "replace";

/** Loading the editor chunk is a code-splitting point of the bundle (S-13): it cannot be a static import. */
const CodeEditorView = lazy(() =>
  import("./editor/codemirror.tsx").then((module) => ({ default: module.CodeEditorView })),
);
const CodeDiffView = lazy(() => import("./editor/codemirror.tsx").then((module) => ({ default: module.CodeDiffView })));

/** What the owner of a `CodeEditor` can ask of it (its `handle`). */
export interface CodeEditorHandle {
  /** The current text. */
  text(): string;
  focus(): void;
}

type RootAttributes = Omit<HTMLAttributes<HTMLDivElement>, "children" | "onChange">;

export interface CodeEditorProps extends RootAttributes {
  /** The text. A new value replaces the document (a reload) and counts as saved. */
  readonly value: string;
  /** The file's path: its extension picks the language (plain text when unknown). */
  readonly path?: string | undefined;
  /** Read-only keeps vim's motions, search and `:` commands; `i` then calls `onRequestEdit`. */
  readonly readOnly: boolean;
  /** 1-based line to put the cursor on and scroll to; `jump` asks for the same line again when it changes. */
  readonly line?: number | undefined;
  readonly jump?: number | undefined;
  /** `:w` or Ctrl-S, with the text to save. */
  readonly onSave?: ((text: string) => void) | undefined;
  /** `:q` (`force`: `:q!`). */
  readonly onQuit?: ((force: boolean) => void) | undefined;
  /** Esc in vim's normal mode with nothing pending: the app moves the focus elsewhere. */
  readonly onLeave?: (() => void) | undefined;
  /** `i` in normal mode while read-only. */
  readonly onRequestEdit?: (() => void) | undefined;
  /** vim's mode while the editor has the focus; `undefined` once it lost it. */
  readonly onModeChange?: ((mode: VimMode | undefined) => void) | undefined;
  /** Whether the text differs from the last `value`. */
  readonly onDirtyChange?: ((dirty: boolean) => void) | undefined;
  /** Filled with the {@link CodeEditorHandle} while the editor is mounted. */
  readonly handle?: RefObject<CodeEditorHandle | null> | undefined;
  /** Shown while the editor chunk loads. */
  readonly loading?: ReactNode;
}

/** A CodeMirror editor with vim bindings, line numbers and highlighting; it owns Esc while it has the focus. */
export function CodeEditor({
  value,
  path,
  readOnly,
  line,
  jump,
  onSave,
  onQuit,
  onLeave,
  onRequestEdit,
  onModeChange,
  onDirtyChange,
  handle,
  loading,
  className,
  "aria-label": label,
  ...props
}: CodeEditorProps): ReactNode {
  return (
    <div {...props} className={cx("omfp-code-editor", className)} data-read-only={readOnly}>
      <Suspense fallback={loading}>
        <CodeEditorView
          value={value}
          path={path}
          readOnly={readOnly}
          line={line}
          jump={jump}
          onSave={onSave}
          onQuit={onQuit}
          onLeave={onLeave}
          onRequestEdit={onRequestEdit}
          onModeChange={onModeChange}
          onDirtyChange={onDirtyChange}
          handle={handle}
          aria-label={label}
        />
      </Suspense>
    </div>
  );
}

export type CodeDiffLayout = "unified" | "split";

export interface CodeDiffProps extends RootAttributes {
  /** The text before. */
  readonly original: string;
  /** The text after. */
  readonly value: string;
  /** The file's path: its extension picks the language. */
  readonly path?: string | undefined;
  /** `unified`: one editor with the deleted lines inline; `split`: old and new side by side. */
  readonly layout: CodeDiffLayout;
  /** Shown while the editor and merge chunks load. */
  readonly loading?: ReactNode;
}

/** A read-only diff of two texts with `@codemirror/merge`. */
export function CodeDiff({ original, value, path, layout, loading, className, ...props }: CodeDiffProps): ReactNode {
  return (
    <section {...props} className={cx("omfp-code-diff", className)} data-layout={layout}>
      <Suspense fallback={loading}>
        <CodeDiffView original={original} value={value} path={path} layout={layout} />
      </Suspense>
    </section>
  );
}
