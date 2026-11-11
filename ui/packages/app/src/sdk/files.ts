/**
 * Files in the SDK (AGT-027, UI-110..117): `useFile` reads a workspace file through the daemon and follows its changes,
 * `useOpenFile` opens one in a file tab of the view's `main` slot (or the frame's floating panel), `useMissingFile`
 * finds the file a path that does not exist names (`workspaces.find_file`, DMN-092), and the window keys the file
 * tabs, the floating panel, the file picker and the mode line read.
 */

import { CallError } from "@omfp/client";
import type { VimMode } from "@omfp/kit";
import type { FileCandidate, FileContent } from "@omfp/protocol";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { strings } from "../strings/index.ts";
import {
  closeTab,
  decodeFloatingPanel,
  decodeOpenFiles,
  type FileTabs,
  FLOATING_PANEL_DEFAULT,
  type FloatingPanel,
  NO_FILE_TABS,
  type OpenFileOptions,
  type OpenFiles,
  openTab,
  retargetTab,
} from "../vm/files.ts";
import { currentWorkspaceId, errorMessage, reportError, useCurrentWorkspaceId } from "./actions.ts";
import { useCall, useWindowState, useWorkspace } from "./hooks.ts";
import { type OmfpHost, useHost } from "./host.tsx";
import { SELECTED_THREAD } from "./keys.ts";
import { windowKey } from "./window-store.ts";

/** The open file tabs of every workspace in this window, kept in `localStorage` (UI-115). */
export const OPEN_FILES = windowKey<OpenFiles>("openFiles", {}, decodeOpenFiles);

/** Where the floating file panel sits and whether it is minimised, kept in `localStorage` (UI-115). */
export const FLOATING_FILES = windowKey<FloatingPanel>("floatingFiles", FLOATING_PANEL_DEFAULT, decodeFloatingPanel);

/** A path that does not exist, with the files it may name, for the frame's file picker (UI-117). */
export interface FilePicker {
  readonly workspaceId: string;
  /** The path as it was opened. */
  readonly path: string;
  readonly options: OpenFileOptions;
  readonly candidates: readonly FileCandidate[];
}

/** The file picker the frame shows; `undefined` when none is open (UI-117). */
export const FILE_PICKER = windowKey<FilePicker | undefined>("filePicker", undefined);

/** vim's mode while a file editor has the focus, for the mode line (UI-112); `undefined` otherwise. */
export const EDITOR_MODE = windowKey<VimMode | undefined>("editorMode", undefined);

/** How often an open file is checked for changes (UI-110). */
export const FILE_POLL_MS = 2000;

/** Runs `check` periodically until the returned function is called; the host's default is every 2 s. */
export type FilePoll = (check: () => void) => () => void;

export const intervalPoll: FilePoll = (check) => {
  const timer = setInterval(check, FILE_POLL_MS);
  return () => clearInterval(timer);
};

/** Opens `path` in this window's current workspace (UI-115); without a workspace it reports why. */
export function openFile(host: OmfpHost, path: string, options: OpenFileOptions = {}): void {
  const workspaceId = currentWorkspaceId(host);
  if (workspaceId === undefined) {
    reportError(host, new Error(strings.shell.files.noWorkspace(path)));
    return;
  }
  host.window.set(OPEN_FILES, (files) => ({
    ...files,
    [workspaceId]: openTab(files[workspaceId] ?? NO_FILE_TABS, path, options),
  }));
}

/** `openFile(path, { line?, diff? })` for this window (AGT-027). */
export function useOpenFile(): (path: string, options?: OpenFileOptions) => void {
  const host = useHost();
  return useMemo(() => (path: string, options?: OpenFileOptions) => openFile(host, path, options), [host]);
}

/** What became of a path that does not exist (UI-117): `opened` its only candidate, `choosing` among several in the
 * frame's picker, `none` found. */
export type MissingOutcome = "opened" | "choosing" | "none";

/**
 * Looks for the file `path` names in the current workspace (`workspaces.find_file`, DMN-092; the selected thread's tool
 * cards rank first): one candidate takes the place of `path`'s tab (or opens), several open the frame's picker.
 */
export async function findMissingFile(
  host: OmfpHost,
  path: string,
  options: OpenFileOptions = {},
): Promise<MissingOutcome> {
  const workspaceId = currentWorkspaceId(host);
  if (workspaceId === undefined) {
    return "none";
  }
  const threadId = host.window.get(SELECTED_THREAD);
  const found = await host.client.call("workspaces.find_file", {
    workspaceId,
    query: path,
    ...(threadId === undefined ? {} : { threadId }),
  });
  const [only] = found.candidates;
  if (only === undefined) {
    return "none";
  }
  if (found.candidates.length === 1) {
    chooseFile(host, workspaceId, path, only.path, options);
    return "opened";
  }
  host.window.set(FILE_PICKER, { workspaceId, path, options, candidates: found.candidates });
  return "choosing";
}

/** Opens `chosen` in place of the tab of `path` (UI-117), closing the picker. */
export function chooseFile(
  host: OmfpHost,
  workspaceId: string,
  path: string,
  chosen: string,
  options: OpenFileOptions = {},
): void {
  host.window.set(FILE_PICKER, undefined);
  host.window.set(OPEN_FILES, (files) => ({
    ...files,
    [workspaceId]: retargetTab(files[workspaceId] ?? NO_FILE_TABS, path, chosen, options),
  }));
}

/** Closes the picker without a choice: the opening is cancelled, so the tab of the missing `path` closes (UI-117). */
export function cancelFilePicker(host: OmfpHost): void {
  const picker = host.window.get(FILE_PICKER);
  if (picker === undefined) {
    return;
  }
  host.window.set(FILE_PICKER, undefined);
  host.window.set(OPEN_FILES, (files) => {
    const tabs = files[picker.workspaceId];
    return tabs === undefined ? files : { ...files, [picker.workspaceId]: closeTab(tabs, picker.path) };
  });
}

/**
 * While `missing` (the file `path` does not exist as given), looks for the file it names once (UI-117): `undefined`
 * until it is missing, then `finding`, then the {@link MissingOutcome}.
 */
export function useMissingFile(
  path: string,
  missing: boolean,
  options: OpenFileOptions = {},
): "finding" | MissingOutcome | undefined {
  const host = useHost();
  const [outcome, setOutcome] = useState<{ readonly path: string; readonly found: MissingOutcome }>();
  const { line, diff } = options;
  useEffect(() => {
    if (!missing) {
      return;
    }
    let live = true;
    findMissingFile(host, path, { line, diff }).then(
      (found) => {
        if (live) {
          setOutcome({ path, found });
        }
      },
      (error: unknown) => {
        if (live) {
          setOutcome({ path, found: "none" });
          reportError(host, error);
        }
      },
    );
    return () => {
      live = false;
    };
  }, [host, path, missing, line, diff]);
  if (!missing) {
    return undefined;
  }
  return outcome?.path === path ? outcome.found : "finding";
}

/** A workspace's file tabs in this window, with what changes them. */
export interface FileTabsState extends FileTabs {
  /** Selects a tab by id (a file's `fileTabId`, a slot child's layout path); `undefined`: the slot's first tab. */
  select(id: string | undefined): void;
  close(path: string): void;
}

export function useFileTabs(workspaceId: string | undefined): FileTabsState {
  const [files, setFiles] = useWindowState(OPEN_FILES);
  const tabs = workspaceId === undefined ? NO_FILE_TABS : (files[workspaceId] ?? NO_FILE_TABS);
  const update = useCallback(
    (change: (tabs: FileTabs) => FileTabs) => {
      if (workspaceId !== undefined) {
        setFiles((all) => {
          const previous = all[workspaceId] ?? NO_FILE_TABS;
          const next = change(previous);
          return next === previous ? all : { ...all, [workspaceId]: next };
        });
      }
    },
    [setFiles, workspaceId],
  );
  return useMemo(
    () => ({
      ...tabs,
      select: (id) => update((current) => (current.active === id ? current : { ...current, active: id })),
      close: (path) => update((current) => closeTab(current, path)),
    }),
    [tabs, update],
  );
}

export type FileLoadStatus = "loading" | "ready" | "missing" | "error";

/** The outcome of {@link FileState.save}. */
export type SaveResult =
  | { readonly ok: true; readonly file: FileContent }
  /** `conflict`: the file no longer has the expected hash (DMN-090); nothing was written. */
  | { readonly ok: false; readonly conflict: boolean; readonly error: string };

export interface FileState {
  readonly status: FileLoadStatus;
  /** The latest version read (`undefined` while loading, missing or failed). */
  readonly file: FileContent | undefined;
  readonly error: string | undefined;
  /** Reads the file again now, `HEAD` text included. */
  reload(): void;
  /** Writes `content` if the file still has `expectedHash` (default: the hash of `file`). */
  save(content: string, expectedHash?: string): Promise<SaveResult>;
}

/** The daemon's error code of a failed call (`not_found`, `conflict`, …); `undefined` for other failures. */
export function errorCode(error: unknown): string | undefined {
  return error instanceof CallError && error.failure.kind === "daemon" ? error.failure.error.code : undefined;
}

interface Loaded {
  readonly status: FileLoadStatus;
  readonly file: FileContent | undefined;
  readonly error: string | undefined;
}

const LOADING: Loaded = { status: "loading", file: undefined, error: undefined };

/**
 * A file of the current workspace (`workspaces.read_file`, DMN-089). With `follow` (the default) it is checked again
 * by the host's poll (`knownHash`, so an unchanged file costs no transfer) and read again whenever the workspace's git
 * status changes (`workspace/<id>`), which also refreshes its `HEAD` text.
 */
export function useFile(path: string, { follow = true }: { readonly follow?: boolean } = {}): FileState {
  const host = useHost();
  const call = useCall();
  const workspaceId = useCurrentWorkspaceId();
  const { status: gitStatus } = useWorkspace(follow ? workspaceId : undefined);
  const [loaded, setLoaded] = useState<Loaded>(LOADING);
  const current = useRef<FileContent | undefined>(undefined);
  // Results of an older path or workspace are dropped.
  const generation = useRef(0);

  const read = useCallback(
    async (check: boolean) => {
      const asked = generation.current;
      if (workspaceId === undefined) {
        setLoaded({ status: "error", file: undefined, error: strings.shell.files.noWorkspace(path) });
        return;
      }
      const known = check ? current.current?.hash : undefined;
      try {
        const file = await call("workspaces.read_file", {
          workspaceId,
          path,
          ...(known === undefined ? {} : { knownHash: known }),
        });
        if (asked !== generation.current || file.unchanged === true) {
          return;
        }
        current.current = file;
        setLoaded({ status: "ready", file, error: undefined });
      } catch (error) {
        if (asked !== generation.current) {
          return;
        }
        if (errorCode(error) === "not_found") {
          current.current = undefined;
          setLoaded({ status: "missing", file: undefined, error: errorMessage(error) });
        } else if (!check) {
          setLoaded({ status: "error", file: current.current, error: errorMessage(error) });
        }
      }
    },
    [call, workspaceId, path],
  );

  useEffect(() => {
    generation.current++;
    current.current = undefined;
    setLoaded(LOADING);
    void read(false);
  }, [read]);

  useEffect(() => (follow ? host.filePoll(() => void read(true)) : undefined), [host, follow, read]);

  // A new git status (an agent's edit, a commit, a rescan): read again, `HEAD` text included.
  const seenStatus = useRef(gitStatus);
  useEffect(() => {
    const seen = seenStatus.current;
    seenStatus.current = gitStatus;
    // The topic's first snapshot is no change: the file was just read.
    if (gitStatus !== seen && seen !== undefined) {
      void read(false);
    }
  }, [gitStatus, read]);

  const save = useCallback(
    async (content: string, expectedHash?: string): Promise<SaveResult> => {
      const file = current.current;
      if (workspaceId === undefined || file === undefined) {
        return { ok: false, conflict: false, error: strings.shell.file.loading };
      }
      try {
        const written = await call("workspaces.write_file", {
          workspaceId,
          path: file.path,
          content,
          expectedHash: expectedHash ?? file.hash,
        });
        const saved: FileContent = {
          ...file,
          content,
          hash: written.hash,
          size: written.size,
          unchanged: false,
          truncated: false,
        };
        current.current = saved;
        setLoaded({ status: "ready", file: saved, error: undefined });
        return { ok: true, file: saved };
      } catch (error) {
        return { ok: false, conflict: errorCode(error) === "conflict", error: errorMessage(error) };
      }
    },
    [call, workspaceId],
  );

  return useMemo(() => ({ ...loaded, reload: () => void read(false), save }), [loaded, read, save]);
}
