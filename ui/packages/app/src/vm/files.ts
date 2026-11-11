/**
 * Files (UI-110..116) as data: the file tabs a window keeps open per workspace, and the text a file had before a
 * patch (what `omfp:changes` diffs the current file against, UI-080). Pure functions, unit tested.
 */

import { parsePatch } from "@omfp/kit";

/** An open file tab (UI-115): the path as it was opened, where to put the cursor, and whether it shows the diff. */
export interface FileTab {
  readonly path: string;
  readonly line?: number | undefined;
  readonly diff?: boolean | undefined;
  /** Bumped when the same file is opened again, so the editor jumps to `line` again. */
  readonly seq: number;
}

/** A workspace's open files in this window and the selected tab (`undefined`: the slot's own first tab). */
export interface FileTabs {
  readonly tabs: readonly FileTab[];
  readonly active: string | undefined;
}

export const NO_FILE_TABS: FileTabs = { tabs: [], active: undefined };

/** Every workspace's {@link FileTabs}, by workspace id (kept in `localStorage`, UI-115). */
export type OpenFiles = Readonly<Record<string, FileTabs>>;

/** Options of `openFile`. */
export interface OpenFileOptions {
  readonly line?: number | undefined;
  readonly diff?: boolean | undefined;
}

/** The tab id of a file in the `main` slot (the slot's own children use their layout path). */
export function fileTabId(path: string): string {
  return `file:${path}`;
}

/** `tabs` with `path` open and selected: a new tab at the end, or the open one with the new `line` and `diff`. */
export function openTab(tabs: FileTabs, path: string, options: OpenFileOptions = {}): FileTabs {
  const open = tabs.tabs.find((tab) => tab.path === path);
  const tab: FileTab = { path, line: options.line, diff: options.diff, seq: (open?.seq ?? 0) + 1 };
  return {
    tabs: open === undefined ? [...tabs.tabs, tab] : tabs.tabs.map((other) => (other === open ? tab : other)),
    active: fileTabId(path),
  };
}

/** `tabs` without `path`; when it was selected, its neighbour (the previous file, else the next, else the slot's own
 * first tab) is. */
export function closeTab(tabs: FileTabs, path: string): FileTabs {
  const index = tabs.tabs.findIndex((tab) => tab.path === path);
  if (index < 0) {
    return tabs;
  }
  const rest = tabs.tabs.filter((_, at) => at !== index);
  if (tabs.active !== fileTabId(path)) {
    return { tabs: rest, active: tabs.active };
  }
  const neighbour = rest[index - 1] ?? rest[index];
  return { tabs: rest, active: neighbour === undefined ? undefined : fileTabId(neighbour.path) };
}

/** Reads a persisted {@link OpenFiles}; `undefined` when it is not one. */
export function decodeOpenFiles(stored: unknown): OpenFiles | undefined {
  if (typeof stored !== "object" || stored === null || Array.isArray(stored)) {
    return undefined;
  }
  const out: Record<string, FileTabs> = {};
  for (const [workspaceId, value] of Object.entries(stored)) {
    if (typeof value !== "object" || value === null) {
      continue;
    }
    const { tabs, active } = value as { tabs?: unknown; active?: unknown };
    if (!Array.isArray(tabs)) {
      continue;
    }
    const decoded: FileTab[] = tabs.flatMap((tab: unknown) => {
      if (typeof tab !== "object" || tab === null) {
        return [];
      }
      const { path, line, diff } = tab as { path?: unknown; line?: unknown; diff?: unknown };
      return typeof path === "string"
        ? [{ path, line: typeof line === "number" ? line : undefined, diff: diff === true ? true : undefined, seq: 0 }]
        : [];
    });
    out[workspaceId] = { tabs: decoded, active: typeof active === "string" ? active : undefined };
  }
  return out;
}

/** The last part of a path, the tab's title. */
export function fileName(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  return trimmed.slice(trimmed.lastIndexOf("/") + 1) || path;
}

/** A repository's file as a path of the workspace: under the repository's entry, or as is for the folder itself. */
export function workspaceFilePath(repo: { readonly name: string; readonly isRoot: boolean }, path: string): string {
  return repo.isRoot ? path : `${repo.name}/${path}`;
}

/** The lines of a text and whether it ends with a line break. */
function lines(text: string): { lines: string[]; newline: boolean } {
  if (text === "") {
    return { lines: [], newline: false };
  }
  const split = text.split("\n");
  const newline = split.at(-1) === "";
  if (newline) {
    split.pop();
  }
  return { lines: split, newline };
}

const HUNK = /^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/;

/**
 * The text a file had before `patch` (one file's `git diff`), given its text now: the patch applied in reverse.
 * `undefined` when the patch does not fit `current` (a context or added line differs, the file changed since) or has
 * no hunk for a change (a binary file). An empty patch means no change.
 */
export function originalFromPatch(current: string, patch: string): string | undefined {
  if (patch.trim() === "") {
    return current;
  }
  const now = lines(current);
  const out: string[] = [];
  // Next line of `current` (1-based) not copied yet.
  let next = 1;
  let hunks = 0;
  // `\ No newline at end of file` after an old line (deleted or context) or after an added one.
  let oldWithoutNewline = false;
  let newWithoutNewline = false;
  let previous: string | undefined;
  const copyUpTo = (last: number): boolean => {
    if (last > now.lines.length) {
      return false;
    }
    for (; next <= last; next++) {
      out.push(now.lines[next - 1] ?? "");
    }
    return true;
  };
  for (const line of parsePatch(patch)) {
    switch (line.kind) {
      case "hunk": {
        hunks++;
        const match = HUNK.exec(line.text);
        const start = Number(match?.[1] ?? 0);
        // `+N,0`: the deleted lines were after line N.
        if (!copyUpTo(match?.[2] === "0" ? start : start - 1)) {
          return undefined;
        }
        break;
      }
      case "context":
      case "add": {
        if (line.newLine !== next || now.lines[next - 1] !== line.text) {
          return undefined;
        }
        if (line.kind === "context") {
          out.push(line.text);
        }
        next++;
        break;
      }
      case "del":
        out.push(line.text);
        break;
      case "note":
        if (previous === "add") {
          newWithoutNewline = true;
        } else {
          oldWithoutNewline = true;
        }
        break;
      default:
        break;
    }
    previous = line.kind;
  }
  if (hunks === 0 || !copyUpTo(now.lines.length)) {
    return undefined;
  }
  // An empty file now (deleted) says nothing about the old one's last line break: only a marker does.
  const newline = !oldWithoutNewline && (newWithoutNewline || now.newline || now.lines.length === 0);
  return out.length === 0 ? "" : `${out.join("\n")}${newline ? "\n" : ""}`;
}
