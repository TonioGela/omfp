/**
 * Opens the files the agent asks this window to open with its `omfp_open_file` tool (DMN-060, UI-116): the daemon names
 * the window the user last used on the thread by its connection number (PRT-051) in the thread's `open_file`; that
 * window selects the requesting thread, then opens the path once as a click in that thread does (`openFile`: the
 * `main` slot's tabs, else the floating panel; a missing or partial path goes through the file picker, UI-117).
 */

import { type ReactNode, useEffect, useRef } from "react";
import { openFile, SELECTED_THREAD, threadActions, useHost, useSnapshot } from "../sdk/index.ts";

export function AgentFiles(): ReactNode {
  const host = useHost();
  const connection = useSnapshot((snapshot) =>
    snapshot.connection.kind === "connected" ? snapshot.connection.hello.connection : undefined,
  );
  const threads = useSnapshot((snapshot) => snapshot.threads);
  // A thread followed again, or its snapshot after a resync, carries the request it already opened.
  const opened = useRef(new Set<string>());
  useEffect(() => {
    for (const [threadId, { state }] of threads) {
      const request = state.open_file;
      if (request === null || request.connection !== connection || opened.current.has(request.id)) {
        continue;
      }
      opened.current.add(request.id);
      if (host.window.get(SELECTED_THREAD) !== threadId) {
        threadActions(host).select(threadId);
      }
      openFile(host, request.path, { line: request.line ?? undefined });
    }
  }, [host, connection, threads]);
  return null;
}
