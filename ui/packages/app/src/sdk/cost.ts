/**
 * What a thread spent (UI-690..UI-692): its own model calls and its subagents', the numbers the header's cost shows.
 */

import { useEffect, useRef, useState } from "react";
import { type ThreadCost, threadCost, untrackedCost } from "../vm/header.ts";
import { useCall, useThread } from "./hooks.ts";

/**
 * The cost of thread `threadId` (`undefined` until its state arrives), following its events: the transcript's
 * assistant messages and `task` results, the subagents' progress, and what omp spent outside the transcript (titles,
 * judgments: `thread.session`'s usage, read while omp runs, on mount and whenever the thread settles, UI-691).
 */
export function useThreadCost(threadId: string): ThreadCost | undefined {
  const state = useThread(threadId)?.state;
  const call = useCall();
  const [untracked, setUntracked] = useState({ threadId, cost: 0 });
  const latest = useRef(state);
  latest.current = state;
  const running = state?.process.state === "running";
  const settled = state?.settled === true;
  useEffect(() => {
    if (!running || !settled) {
      return;
    }
    let live = true;
    call("thread.session", { threadId }).then(
      (session) => {
        const current = latest.current;
        if (live && current !== undefined && session.usage != null) {
          setUntracked({ threadId, cost: untrackedCost(session.usage.cost, current) });
        }
      },
      // Without it the cost lacks only what omp spent outside the transcript.
      () => {},
    );
    return () => {
      live = false;
    };
  }, [call, threadId, running, settled]);
  return state === undefined ? undefined : threadCost(state, untracked.threadId === threadId ? untracked.cost : 0);
}
