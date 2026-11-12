/**
 * The `omfp:tree` widget (UI-083): omp's session tree (`thread.tree`: `{ tree, leafId }`, each node
 * `{ entry, children, label? }`) as rows. Only conversation entries show — user and assistant messages, branch and
 * compaction summaries —; the others (model and thinking changes, tool results, custom records) pass their children
 * up. A chain stays at its depth; the branches of an entry with several children are indented one step.
 */

import { isObject } from "../sdk/json.ts";
import { strings } from "../strings/index.ts";

const text = strings.shell.tree;

/** The longest text a row shows (its first line). */
const ROW_TEXT = 160;

export interface TreeRow {
  readonly id: string;
  readonly depth: number;
  readonly kind: "user" | "assistant" | "summary";
  /** The first line of the entry's text (tool names for an assistant message without text). */
  readonly text: string;
  readonly label: string | undefined;
  /** On the branch from the root to the session's leaf: what the transcript shows. */
  readonly active: boolean;
  /** The session's leaf, or the deepest shown entry above it: where the next message goes. */
  readonly leaf: boolean;
  /** A message entry: omp forks only from those. */
  readonly forkable: boolean;
}

export interface TreeView {
  readonly rows: readonly TreeRow[];
  readonly leafId: string | undefined;
}

interface Shown {
  readonly id: string;
  readonly kind: TreeRow["kind"];
  readonly text: string;
  readonly label: string | undefined;
  readonly forkable: boolean;
  readonly children: readonly Shown[];
}

function firstLine(value: string): string {
  const line = value.trim().split("\n", 1)[0] ?? "";
  return line.length > ROW_TEXT ? `${line.slice(0, ROW_TEXT - 1)}…` : line;
}

/** The text of a message content: the string, or its text blocks joined by newlines. */
export function contentText(content: unknown): string {
  if (typeof content === "string") {
    return content;
  }
  return (Array.isArray(content) ? content : [])
    .flatMap((block: unknown) =>
      isObject(block) && block.type === "text" && typeof block.text === "string" ? [block.text] : [],
    )
    .join("\n");
}

/** What a shown entry is, or `undefined` for one that passes its children up. */
function shown(entry: Readonly<Record<string, unknown>>): Pick<Shown, "kind" | "text" | "forkable"> | undefined {
  switch (entry.type) {
    case "message": {
      const message = isObject(entry.message) ? entry.message : {};
      if (message.role === "user") {
        return { kind: "user", text: firstLine(contentText(message.content)), forkable: true };
      }
      if (message.role !== "assistant") {
        return undefined;
      }
      const said = firstLine(contentText(message.content));
      const tools = (Array.isArray(message.content) ? message.content : []).flatMap((block: unknown) =>
        isObject(block) && block.type === "toolCall" && typeof block.name === "string" ? [block.name] : [],
      );
      return { kind: "assistant", text: said !== "" ? said : text.tools(tools), forkable: true };
    }
    case "branch_summary":
      return { kind: "summary", text: text.branchSummary(firstLine(String(entry.summary ?? ""))), forkable: false };
    case "compaction":
      return { kind: "summary", text: text.compaction(firstLine(String(entry.summary ?? ""))), forkable: false };
    default:
      return undefined;
  }
}

/** The shown nodes of `nodes`, hidden ones replaced by their shown children; fills `parents` (id → parent id). */
function collect(nodes: unknown, parents: Map<string, string | null>): Shown[] {
  return (Array.isArray(nodes) ? nodes : []).flatMap((node: unknown): Shown[] => {
    if (!isObject(node) || !isObject(node.entry) || typeof node.entry.id !== "string") {
      return [];
    }
    const { entry } = node;
    const id = node.entry.id;
    parents.set(id, typeof entry.parentId === "string" ? entry.parentId : null);
    const children = collect(node.children, parents);
    const what = shown(entry);
    if (what === undefined) {
      return children;
    }
    return [{ id, ...what, label: typeof node.label === "string" ? node.label : undefined, children }];
  });
}

export function treeView(result: unknown): TreeView {
  const parents = new Map<string, string | null>();
  const roots = collect(isObject(result) ? result.tree : undefined, parents);
  const leafId = isObject(result) && typeof result.leafId === "string" ? result.leafId : undefined;
  const active = new Set<string>();
  for (let id: string | null | undefined = leafId; typeof id === "string" && !active.has(id); id = parents.get(id)) {
    active.add(id);
  }
  const rows: TreeRow[] = [];
  const walk = (nodes: readonly Shown[], depth: number) => {
    for (const node of nodes) {
      rows.push({
        id: node.id,
        depth,
        kind: node.kind,
        text: node.text,
        label: node.label,
        active: active.has(node.id),
        leaf: false,
        forkable: node.forkable,
      });
      walk(node.children, node.children.length > 1 ? depth + 1 : depth);
    }
  };
  walk(roots, 0);
  // The leaf itself may be hidden (a label or model change): the deepest shown entry of its branch stands in.
  const last = rows.findLastIndex((row) => row.active);
  const leaf = rows[last];
  if (leaf !== undefined) {
    rows[last] = { ...leaf, leaf: true };
  }
  return { rows, leafId };
}

/** What a `thread.navigate` result says: nothing moved, or the leaf moved, with the text of the user message navigated
 * to (for the composer) when the target was one. */
export type NavigateOutcome =
  | { readonly kind: "unchanged" }
  | { readonly kind: "moved"; readonly editorText: string | undefined };

export function navigateOutcome(result: unknown): NavigateOutcome {
  if (!isObject(result) || result.cancelled === true || result.aborted === true) {
    return { kind: "unchanged" };
  }
  return { kind: "moved", editorText: typeof result.editorText === "string" ? result.editorText : undefined };
}

/** The composer's text once a navigation gave `editorText` back (UI-083): that text, after a blank line when the
 * draft already had some, so nothing typed is lost. */
export function withEditorText(draft: string, editorText: string): string {
  return draft.trim() === "" ? editorText : `${draft.replace(/\s+$/, "")}\n\n${editorText}`;
}
