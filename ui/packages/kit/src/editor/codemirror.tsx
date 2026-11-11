/**
 * The CodeMirror chunk of the kit (S-13), loaded on the first `CodeEditor` or `CodeDiff`: one `EditorView` per mount
 * whose props reconfigure it through compartments, never remount. The merge view is its own chunk again, loaded only
 * by diffs.
 */
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { bracketMatching } from "@codemirror/language";
import { searchKeymap } from "@codemirror/search";
import { Compartment, EditorSelection, EditorState, type Extension, Prec, type Text } from "@codemirror/state";
import {
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from "@codemirror/view";
import { type CodeMirrorV, getCM, Vim, vim } from "@replit/codemirror-vim";
import { type ReactNode, useEffect, useRef } from "react";
import type { CodeDiffProps, CodeEditorProps, VimMode } from "../editor.tsx";
import { languageExtension } from "./languages.ts";
import { omfpHighlight, omfpTheme } from "./theme.ts";

type Callbacks = Pick<
  CodeEditorProps,
  "onSave" | "onQuit" | "onLeave" | "onRequestEdit" | "onModeChange" | "onDirtyChange"
>;

/** The current callbacks of each view, read by the vim ex commands (global to the vim plugin). */
const callbacks = new WeakMap<EditorView, { current: Callbacks }>();

function viewOf(cm: CodeMirrorV): EditorView {
  return cm.cm6 as EditorView;
}

Vim.defineEx("write", "w", (cm: CodeMirrorV) => {
  const view = viewOf(cm);
  callbacks.get(view)?.current.onSave?.(view.state.doc.toString());
});
Vim.defineEx("quit", "q", (cm: CodeMirrorV, params: { argString?: string }) => {
  callbacks.get(viewOf(cm))?.current.onQuit?.(params.argString?.trim() === "!");
});

/** True when vim is in normal mode with nothing pending (no operator, count, register or key sequence). */
function vimIdle(view: EditorView): boolean {
  const state = (getCM(view) as CodeMirrorV | null)?.state.vim;
  if (!state) {
    return true;
  }
  const input = state.inputState;
  return (
    !state.insertMode &&
    !state.visualMode &&
    !state.exMode &&
    input.keyBuffer.length === 0 &&
    !input.operator &&
    input.prefixRepeat.length === 0 &&
    input.motionRepeat.length === 0 &&
    !input.registerName &&
    !state.status
  );
}

/** The vim plugin instance @replit/codemirror-vim keeps on its CodeMirror adapter (untyped in its d.ts). */
type VimPluginState = { vimPlugin?: { handleKey(event: KeyboardEvent, view: EditorView): void } };

/**
 * Runs before vim's own keydown handler (Prec.highest). The editor owns every Esc (UI-111): in idle normal mode it
 * leaves the editor (the app decides where focus goes), otherwise vim gets it (insert → normal, cancel a pending
 * command) and it is marked handled, so the app's insert-mode Esc (`focus.leave`) never sees it. `i` on a read-only
 * file asks the app to switch to editing.
 */
function appKeys(readOnly: () => boolean): Extension {
  return Prec.highest(
    EditorView.domEventHandlers({
      keydown(event, view) {
        if (event.ctrlKey || event.altKey || event.metaKey) {
          return false;
        }
        const cb = callbacks.get(view)?.current;
        if (event.key === "Escape") {
          if (vimIdle(view)) {
            cb?.onLeave?.();
          } else {
            // vim's own key path (status line, block cursor, mode events), as if its handler had run first.
            (getCM(view)?.state as VimPluginState | undefined)?.vimPlugin?.handleKey(event, view);
          }
          event.preventDefault();
          return true;
        }
        if (event.key === "i" && readOnly() && vimIdle(view) && cb?.onRequestEdit !== undefined) {
          event.preventDefault();
          cb.onRequestEdit();
          return true;
        }
        return false;
      },
    }),
  );
}

/** Moves the cursor to 1-based `line` (clamped) and scrolls it to the middle. */
function jumpTo(view: EditorView, line: number): void {
  const target = view.state.doc.line(Math.min(Math.max(1, line), view.state.doc.lines));
  view.dispatch({
    selection: EditorSelection.cursor(target.from),
    effects: EditorView.scrollIntoView(target.from, { y: "center" }),
  });
}

/** The editor of `CodeEditor`: see its props. */
export function CodeEditorView(props: CodeEditorProps): ReactNode {
  const host = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const cbRef = useRef<Callbacks>(props);
  cbRef.current = props;
  const readOnlyRef = useRef(props.readOnly);
  readOnlyRef.current = props.readOnly;
  // The text the document is compared with to tell whether it has unsaved changes.
  const base = useRef<Text | null>(null);
  const dirty = useRef(false);
  // vim's mode, from its `vim-mode-change` events.
  const mode = useRef<VimMode>("normal");
  const compartments = useRef({ language: new Compartment(), readOnly: new Compartment() });
  const { value, path, readOnly, line, jump } = props;

  // Mount once; the props below flow in through compartments and transactions.
  // biome-ignore lint/correctness/useExhaustiveDependencies: mount once, see above
  useEffect(() => {
    const parent = host.current;
    if (parent === null) {
      return;
    }
    const setDirty = (next: boolean) => {
      if (next !== dirty.current) {
        dirty.current = next;
        cbRef.current.onDirtyChange?.(next);
      }
    };
    const view = new EditorView({
      parent,
      state: EditorState.create({
        doc: value,
        extensions: [
          appKeys(() => readOnlyRef.current),
          // vim() before the other keymaps (its README); its panel also hosts the `:` prompt.
          vim({ status: true }),
          lineNumbers(),
          highlightActiveLineGutter(),
          highlightActiveLine(),
          drawSelection(),
          history(),
          bracketMatching(),
          omfpHighlight,
          omfpTheme,
          keymap.of([
            {
              key: "Mod-s",
              run: (target) => {
                cbRef.current.onSave?.(target.state.doc.toString());
                return true;
              },
            },
            ...defaultKeymap,
            ...historyKeymap,
            ...searchKeymap,
          ]),
          compartments.current.language.of([]),
          compartments.current.readOnly.of(EditorState.readOnly.of(readOnly)),
          EditorView.contentAttributes.of({ "aria-label": props["aria-label"] ?? "" }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged && base.current !== null) {
              setDirty(!update.state.doc.eq(base.current));
            }
            if (update.focusChanged) {
              cbRef.current.onModeChange?.(update.view.hasFocus ? mode.current : undefined);
            }
          }),
        ],
      }),
    });
    base.current = view.state.doc;
    callbacks.set(view, cbRef);
    getCM(view)?.on("vim-mode-change", (event: { mode: VimMode }) => {
      mode.current = event.mode;
      if (view.hasFocus) {
        cbRef.current.onModeChange?.(event.mode);
      }
    });
    viewRef.current = view;
    if (props.handle !== undefined) {
      props.handle.current = { text: () => view.state.doc.toString(), focus: () => view.focus() };
    }
    if (line !== undefined) {
      jumpTo(view, line);
    }
    return () => {
      if (view.hasFocus) {
        cbRef.current.onModeChange?.(undefined);
      }
      view.destroy();
      viewRef.current = null;
      if (props.handle !== undefined) {
        props.handle.current = null;
      }
    };
  }, []);

  // A new `value` replaces the document (a reload or a discard) and becomes the text without unsaved changes.
  useEffect(() => {
    const view = viewRef.current;
    if (view === null || base.current === null) {
      return;
    }
    // The new base first, so the replacement itself is no unsaved change.
    base.current = view.state.toText(value);
    if (!view.state.doc.eq(base.current)) {
      const head = Math.min(view.state.selection.main.head, value.length);
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value }, selection: { anchor: head } });
    }
    if (dirty.current) {
      dirty.current = false;
      cbRef.current.onDirtyChange?.(false);
    }
  }, [value]);

  useEffect(() => {
    let live = true;
    void languageExtension(path).then((extension) => {
      if (live) {
        viewRef.current?.dispatch({ effects: compartments.current.language.reconfigure(extension) });
      }
    });
    return () => {
      live = false;
    };
  }, [path]);

  useEffect(() => {
    const view = viewRef.current;
    if (view === null) {
      return;
    }
    view.dispatch({ effects: compartments.current.readOnly.reconfigure(EditorState.readOnly.of(readOnly)) });
    const cm = getCM(view);
    if (!readOnly && cm !== null) {
      // Editing starts in insert mode, as vim's `i` would, with the focus in the editor.
      view.focus();
      if (vimIdle(view)) {
        Vim.handleKey(cm as CodeMirrorV, "i", "user");
      }
    } else if (readOnly && cm !== null && !vimIdle(view)) {
      Vim.handleKey(cm as CodeMirrorV, "<Esc>", "user");
    }
  }, [readOnly]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `jump` asks for the same line again
  useEffect(() => {
    const view = viewRef.current;
    if (view !== null && line !== undefined) {
      jumpTo(view, line);
    }
  }, [line, jump]);

  return <div ref={host} className="omfp-code-editor__view" />;
}

/** The merge view of `CodeDiff`: see its props. A layout switch rebuilds it (another DOM structure). */
export function CodeDiffView({ original, value, path, layout }: CodeDiffProps): ReactNode {
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let destroy = () => {};
    let live = true;
    // Merge is its own chunk: only diffs pay for it.
    void Promise.all([import("@codemirror/merge"), languageExtension(path)]).then(([merge, language]) => {
      const parent = host.current;
      if (!live || parent === null) {
        return;
      }
      const shown = [
        EditorState.readOnly.of(true),
        EditorView.editable.of(false),
        lineNumbers(),
        omfpHighlight,
        omfpTheme,
        language,
      ];
      if (layout === "unified") {
        const view = new EditorView({
          parent,
          doc: value,
          extensions: [shown, merge.unifiedMergeView({ original, mergeControls: false, gutter: true })],
        });
        destroy = () => view.destroy();
      } else {
        const view = new merge.MergeView({
          parent,
          a: { doc: original, extensions: shown },
          b: { doc: value, extensions: shown },
          gutter: true,
        });
        destroy = () => view.destroy();
      }
    });
    return () => {
      live = false;
      destroy();
    };
  }, [original, value, path, layout]);
  return <div ref={host} className="omfp-code-diff__view" data-layout={layout} />;
}
