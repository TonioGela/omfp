/**
 * The tree viewer of `omfp:file` (UI-567..569): a JSON, JSONC or YAML file as a tree of its values — keys, types,
 * counts — whose containers open on a click, each node copying its path or its value; a file that does not parse is
 * handed back to the source with its error.
 */

import { parseYaml, strings, useReportError, useReportSuccess, type YamlResult } from "omfp/sdk";
import { type ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import { fromValue, isJsonc, type ParseError, parseJson, pathText, type TreeValue, valueText } from "./structured.ts";
import type { ViewerProps } from "./viewers.tsx";

const text = strings.shell.file;

/** Children shown at a time under an open container; more on demand. */
const PAGE = 200;

/** A parsed file: its value, and how many YAML documents it holds when several (the root is then their list). */
type Tree =
  | { readonly ok: true; readonly value: TreeValue; readonly documents: number }
  | { readonly ok: false; readonly error: ParseError };

/** The tree of YAML `content`, parsed off the render (the parser is a chunk loaded on first use); `undefined` until then. */
function useYamlTree(content: string | undefined): Tree | undefined {
  const [tree, setTree] = useState<{ content: string; tree: Tree } | undefined>(undefined);
  useEffect(() => {
    if (content === undefined) {
      return;
    }
    let live = true;
    void parseYaml(content).then((result) => {
      if (live) {
        setTree({ content, tree: yamlTree(result) });
      }
    });
    return () => {
      live = false;
    };
  }, [content]);
  return tree !== undefined && tree.content === content ? tree.tree : undefined;
}

function yamlTree(result: YamlResult): Tree {
  if (!result.ok) {
    return { ok: false, error: result };
  }
  try {
    const [first] = result.documents;
    const several = result.documents.length > 1;
    return { ok: true, value: fromValue(several ? result.documents : first), documents: result.documents.length };
  } catch (thrown) {
    // A recursive alias nests without end.
    if (thrown instanceof RangeError) {
      return { ok: false, error: { message: thrown.message, line: 1, column: 1 } };
    }
    throw thrown;
  }
}

function TreeViewer({ file, content, onError, yaml }: ViewerProps & { readonly yaml: boolean }): ReactNode {
  const json = useMemo<Tree | undefined>(
    () => (yaml ? undefined : { documents: 1, ...parseJson(content, isJsonc(file.path)) }),
    [yaml, content, file.path],
  );
  const parsedYaml = useYamlTree(yaml ? content : undefined);
  const tree = json ?? parsedYaml;
  useEffect(() => {
    if (tree?.ok === false) {
      onError({
        message: text.treeFailed(tree.error.message, tree.error.line, tree.error.column),
        line: tree.error.line,
      });
    }
  }, [tree, onError]);
  const reportSuccess = useReportSuccess();
  const reportError = useReportError();
  const copy = useCallback(
    (copied: string, done: string) => {
      // `navigator.clipboard` is missing outside secure contexts: the throw becomes the rejection.
      Promise.resolve()
        .then(() => navigator.clipboard.writeText(copied))
        .then(
          () => reportSuccess(done),
          (error: unknown) => reportError(error),
        );
    },
    [reportSuccess, reportError],
  );
  if (tree === undefined || !tree.ok) {
    return <p className="omfp-file__notice">{text.loading}</p>;
  }
  const name = file.path.slice(file.path.lastIndexOf("/") + 1);
  return (
    <ul className="omfp-file__tree" data-testid="file-tree" aria-label={text.treeLabel(file.path)}>
      <TreeNode label={name} value={tree.value} path={[]} open={true} documents={tree.documents > 1} onCopy={copy} />
    </ul>
  );
}

interface TreeNodeProps {
  readonly label: string;
  readonly value: TreeValue;
  readonly path: readonly (string | number)[];
  /** Open at first. */
  readonly open: boolean;
  /** The root list of a YAML file's documents. */
  readonly documents?: boolean;
  readonly onCopy: (copied: string, done: string) => void;
}

/** What a node says of itself: a container's count, a scalar's type. */
function summary(value: TreeValue, documents: boolean): string {
  switch (value.kind) {
    case "object":
      return text.treeKeys(value.entries.length);
    case "array":
      return documents ? text.treeDocuments(value.items.length) : text.treeItems(value.items.length);
    default:
      return value.kind;
  }
}

function TreeNode({ label, value, path, open: initiallyOpen, documents = false, onCopy }: TreeNodeProps): ReactNode {
  const [open, setOpen] = useState(initiallyOpen);
  const [shown, setShown] = useState(PAGE);
  const where = pathText(path);
  const children: readonly (readonly [string | number, TreeValue])[] =
    value.kind === "object"
      ? value.entries
      : value.kind === "array"
        ? value.items.map((item, index) => [index, item])
        : [];
  const container = value.kind === "object" || value.kind === "array";
  return (
    <li className="omfp-file__tree-item" data-testid="file-tree-node" data-path={where} data-kind={value.kind}>
      <div className="omfp-file__node">
        {container ? (
          <button
            type="button"
            className="omfp-file__key"
            aria-expanded={open}
            onClick={() => setOpen((now) => !now)}
            data-testid="file-tree-toggle"
          >
            {label}
          </button>
        ) : (
          <>
            <span className="omfp-file__key">{label}</span>
            <span className="omfp-file__value" data-kind={value.kind}>
              {value.kind === "string" ? JSON.stringify(value.value) : valueText(value)}
            </span>
          </>
        )}
        <span className="omfp-file__summary">{summary(value, documents)}</span>
        <span className="omfp-file__node-actions">
          <button
            type="button"
            aria-label={text.copyPathLabel(where)}
            onClick={() => onCopy(where, text.copiedPath(where))}
            data-testid="file-tree-copy-path"
          >
            {text.copyPath}
          </button>
          <button
            type="button"
            aria-label={text.copyValueLabel(where)}
            onClick={() => onCopy(valueText(value), text.copiedValue(where))}
            data-testid="file-tree-copy-value"
          >
            {text.copyValue}
          </button>
        </span>
      </div>
      {container && open && children.length > 0 && (
        <ul>
          {children.slice(0, shown).map(([key, child]) => (
            <TreeNode key={key} label={String(key)} value={child} path={[...path, key]} open={false} onCopy={onCopy} />
          ))}
          {children.length > shown && (
            <li>
              <button
                type="button"
                className="omfp-file__more"
                onClick={() => setShown((count) => count + PAGE)}
                data-testid="file-tree-more"
              >
                {text.treeMore(children.length - shown)}
              </button>
            </li>
          )}
        </ul>
      )}
    </li>
  );
}

export function JsonViewer(props: ViewerProps): ReactNode {
  return <TreeViewer {...props} yaml={false} />;
}

export function YamlViewer(props: ViewerProps): ReactNode {
  return <TreeViewer {...props} yaml={true} />;
}
