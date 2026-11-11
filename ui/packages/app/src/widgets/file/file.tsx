/**
 * `omfp:file { path, line?, diff? }` (UI-110..114): a workspace file in CodeMirror, read-only until `i` or Edit, then
 * edited with vim bindings and saved with `:w` / Ctrl-S against the hash the text was based on; a refused save shows
 * the file on disk against the user's text and writes nothing until the user decides. `diff` shows the whole file
 * against `HEAD`, unified or side by side. The text follows the file on disk while it has no unsaved change.
 */

import type { FileContent } from "@omfp/protocol";
import {
  Button,
  Chip,
  CodeDiff,
  type CodeDiffLayout,
  CodeEditor,
  type CodeEditorHandle,
  EmptyState,
  Icon,
  type VimMode,
} from "omfp/kit";
import {
  EDITOR_MODE,
  type FileState,
  strings,
  useAnnounce,
  useFile,
  useMissingFile,
  useReportError,
  useWindowState,
  type WidgetProps,
} from "omfp/sdk";
import { type ReactNode, type RefObject, useCallback, useEffect, useRef, useState } from "react";

const text = strings.shell.file;

/** The version the editor's text is based on: what saves compare with (DMN-090). */
interface Base {
  readonly content: string;
  readonly hash: string;
}

type Mode = "view" | "edit" | "conflict";

export function FileWidget({ path, line, diff, jump }: WidgetProps): ReactNode {
  if (typeof path !== "string" || path === "") {
    return <EmptyState icon="file" title={text.noPath} className="omfp-file__empty" data-testid="file-none" />;
  }
  return (
    <FileView
      path={path}
      line={typeof line === "number" ? line : undefined}
      jump={typeof jump === "number" ? jump : undefined}
      diff={diff === true}
    />
  );
}

interface FileViewProps {
  readonly path: string;
  readonly line: number | undefined;
  /** Changes when the file is opened again, to jump to `line` again. */
  readonly jump: number | undefined;
  readonly diff: boolean;
}

function FileView({ path, line, jump, diff }: FileViewProps): ReactNode {
  const file = useFile(path);
  // A path that does not exist as given may name a file of the workspace (`theme.ts`, `src/theme.ts`, UI-117).
  const finding = useMissingFile(path, file.status === "missing", { line, diff });
  const root = useRef<HTMLElement>(null);
  return (
    <section
      ref={root}
      className="omfp-file"
      data-testid="file"
      data-path={path}
      data-status={file.status}
      aria-label={text.label(path)}
      tabIndex={-1}
    >
      {file.status === "loading" ? (
        <p className="omfp-file__notice">{text.loading}</p>
      ) : file.status === "missing" ? (
        finding === "finding" || finding === "opened" ? (
          <p className="omfp-file__notice" data-testid="file-finding">
            {text.finding(path)}
          </p>
        ) : finding === "choosing" ? (
          <p className="omfp-file__notice" data-testid="file-choosing">
            {text.choosing(path)}
          </p>
        ) : (
          <p className="omfp-file__notice" data-testid="file-missing">
            {text.missing(path)}
          </p>
        )
      ) : file.file === undefined ? (
        <p className="omfp-file__notice" data-tone="error" data-testid="file-error">
          {text.failed(file.error ?? "")}
        </p>
      ) : file.file.kind === "text" && typeof file.file.content === "string" ? (
        <TextFile
          file={file.file}
          content={file.file.content}
          line={line}
          jump={jump}
          diff={diff}
          save={file.save}
          reload={file.reload}
          root={root}
        />
      ) : (
        <OtherFile file={file.file} />
      )}
    </section>
  );
}

/** An image, or a binary file named with its size. */
function OtherFile({ file }: { file: FileContent }): ReactNode {
  if (file.kind === "image" && typeof file.content === "string") {
    return (
      <div className="omfp-file__image" data-testid="file-image">
        <img src={`data:${file.mime ?? "image/png"};base64,${file.content}`} alt={file.path} />
      </div>
    );
  }
  return (
    <EmptyState
      icon="file"
      title={file.kind === "image" ? text.imageTooLarge(text.size(file.size)) : text.binary(text.size(file.size))}
      className="omfp-file__empty"
      data-testid="file-binary"
    />
  );
}

interface TextFileProps {
  readonly file: FileContent;
  readonly content: string;
  readonly line: number | undefined;
  readonly jump: number | undefined;
  readonly diff: boolean;
  readonly save: FileState["save"];
  readonly reload: () => void;
  readonly root: RefObject<HTMLElement | null>;
}

function TextFile({ file, content, line, jump, diff, save, reload, root }: TextFileProps): ReactNode {
  const announce = useAnnounce();
  const reportError = useReportError();
  const [, setEditorMode] = useWindowState(EDITOR_MODE);
  const editor = useRef<CodeEditorHandle | null>(null);
  const [mode, setMode] = useState<Mode>("view");
  const [showDiff, setShowDiff] = useState(diff);
  const [layout, setLayout] = useState<CodeDiffLayout>("unified");
  const [dirty, setDirty] = useState(false);
  const [base, setBase] = useState<Base>({ content, hash: file.hash });
  // The user's text a refused save held (the conflict view's right side).
  const [mine, setMine] = useState<string | undefined>(undefined);
  const [notice, setNotice] = useState<string | undefined>(undefined);
  const editable = file.truncated !== true;

  useEffect(() => setShowDiff(diff), [diff]);

  // The file on disk changed: follow it while nothing is unsaved (UI-110).
  useEffect(() => {
    if (!dirty && mode !== "conflict") {
      setBase((current) => (current.hash === file.hash ? current : { content, hash: file.hash }));
    }
  }, [content, file.hash, dirty, mode]);

  const leave = useCallback(() => root.current?.focus(), [root]);
  const onMode = useCallback((next: VimMode | undefined) => setEditorMode(next), [setEditorMode]);
  useEffect(() => () => setEditorMode(undefined), [setEditorMode]);

  const write = useCallback(
    async (textToSave: string, expectedHash: string) => {
      const result = await save(textToSave, expectedHash);
      if (result.ok) {
        setBase({ content: textToSave, hash: result.file.hash });
        setMine(undefined);
        setNotice(undefined);
        setMode("edit");
        announce(text.saved(result.file.path));
      } else if (result.conflict) {
        setMine(textToSave);
        setMode("conflict");
        reload();
      } else {
        reportError(new Error(text.saveFailed(result.error)));
      }
    },
    [save, reload, announce, reportError],
  );

  const onSave = useCallback((textToSave: string) => void write(textToSave, base.hash), [write, base.hash]);

  const quit = useCallback(
    (force: boolean) => {
      if (dirty && !force) {
        setNotice(text.unsaved);
        return;
      }
      if (dirty) {
        setBase({ content, hash: file.hash });
      }
      setNotice(undefined);
      setMode("view");
      leave();
    },
    [dirty, content, file.hash, leave],
  );

  const editing = mode !== "view";
  const showEditor = mode !== "view" || !showDiff;
  return (
    <>
      <div className="omfp-file__bar" data-testid="file-bar">
        <span className="omfp-file__path">
          <Icon name="file" className="omfp-file__icon" />
          {typeof file.repo === "string" && <Chip>{file.repo}</Chip>}
          <span data-testid="file-path">{file.path}</span>
        </span>
        <span className="omfp-file__actions">
          {mode === "view" && (
            <Button
              size="sm"
              variant="quiet"
              aria-pressed={showDiff}
              onPress={() => setShowDiff((on) => !on)}
              data-testid="file-diff"
            >
              {text.diff}
            </Button>
          )}
          {mode === "view" && showDiff && (
            <Button
              size="sm"
              variant="quiet"
              onPress={() => setLayout((current) => (current === "unified" ? "split" : "unified"))}
              data-testid="file-layout"
            >
              {layout === "unified" ? text.split : text.unified}
            </Button>
          )}
          {mode === "view" && editable && (
            <Button size="sm" onPress={() => setMode("edit")} data-testid="file-edit">
              {text.edit}
            </Button>
          )}
          {mode === "edit" && (
            <>
              <Button
                size="sm"
                variant="primary"
                onPress={() => onSave(editor.current?.text() ?? base.content)}
                data-testid="file-save"
              >
                {text.save}
              </Button>
              <Button size="sm" onPress={() => quit(false)} data-testid="file-done">
                {text.done}
              </Button>
            </>
          )}
        </span>
      </div>
      {file.truncated === true && <p className="omfp-file__notice">{text.truncated(text.size(file.size))}</p>}
      {mode === "edit" && dirty && file.hash !== base.hash && (
        <p className="omfp-file__notice" data-tone="warning" data-testid="file-changed">
          {text.changedOnDisk}
        </p>
      )}
      {notice !== undefined && (
        <p className="omfp-file__notice" data-tone="warning" data-testid="file-notice">
          {notice}
        </p>
      )}
      {mode === "conflict" && mine !== undefined && (
        <Conflict
          path={file.path}
          disk={content}
          mine={mine}
          onKeep={() => {
            setBase({ content: mine, hash: file.hash });
            setMine(undefined);
            setMode("edit");
          }}
          onOverwrite={() => void write(mine, file.hash)}
          onDiscard={() => {
            setBase({ content, hash: file.hash });
            setMine(undefined);
            setMode("view");
          }}
        />
      )}
      {mode === "view" && showDiff && <HeadDiff file={file} content={content} layout={layout} />}
      {showEditor && (
        <CodeEditor
          className="omfp-file__editor"
          hidden={mode === "conflict"}
          value={base.content}
          path={file.path}
          readOnly={!editing}
          line={line}
          jump={jump}
          handle={editor}
          aria-label={text.editorLabel(file.path)}
          data-testid="file-editor"
          data-mode={mode}
          data-dirty={dirty}
          loading={<p className="omfp-file__notice">{text.loadingEditor}</p>}
          onSave={onSave}
          onQuit={quit}
          onLeave={leave}
          onRequestEdit={editable ? () => setMode("edit") : undefined}
          onModeChange={onMode}
          onDirtyChange={setDirty}
        />
      )}
    </>
  );
}

/** The whole file against its `HEAD` text (UI-114). */
function HeadDiff({
  file,
  content,
  layout,
}: {
  file: FileContent;
  content: string;
  layout: CodeDiffLayout;
}): ReactNode {
  const outside = typeof file.repo !== "string";
  return (
    <>
      {(outside || typeof file.head !== "string") && (
        <p className="omfp-file__notice" data-testid="file-head-note">
          {outside ? text.noRepository : text.newFile}
        </p>
      )}
      <CodeDiff
        className="omfp-file__diff"
        original={typeof file.head === "string" ? file.head : ""}
        value={content}
        path={file.path}
        layout={layout}
        aria-label={text.diffLabel(file.path)}
        data-testid="file-head-diff"
        loading={<p className="omfp-file__notice">{text.loadingEditor}</p>}
      />
    </>
  );
}

interface ConflictProps {
  readonly path: string;
  readonly disk: string;
  readonly mine: string;
  onKeep(): void;
  onOverwrite(): void;
  onDiscard(): void;
}

/** A refused save (UI-113): the file on disk against the user's text, and the user's choice. */
function Conflict({ path, disk, mine, onKeep, onOverwrite, onDiscard }: ConflictProps): ReactNode {
  return (
    <div className="omfp-file__conflict" data-testid="file-conflict">
      <p className="omfp-file__notice" data-tone="error" role="alert">
        {text.conflict}
      </p>
      <div className="omfp-file__actions">
        <Button size="sm" onPress={onKeep} data-testid="file-keep">
          {text.keepEditing}
        </Button>
        <Button size="sm" variant="danger" onPress={onOverwrite} data-testid="file-overwrite">
          {text.overwrite}
        </Button>
        <Button size="sm" onPress={onDiscard} data-testid="file-discard">
          {text.discard}
        </Button>
      </div>
      <CodeDiff
        className="omfp-file__diff"
        original={disk}
        value={mine}
        path={path}
        layout="split"
        aria-label={text.conflictLabel(path)}
        data-testid="file-conflict-diff"
        loading={<p className="omfp-file__notice">{text.loadingEditor}</p>}
      />
    </div>
  );
}
