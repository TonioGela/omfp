/** A thread row's state mark (UI-137, UI-169) and its attention motion (UI-146). */

import type { ThreadState } from "@omfp/kit";
import { useEffect, useRef, useState } from "react";
import type { ThreadStatus } from "../../vm/threads.ts";

/** The kit state of a thread: running, awaiting, failed; else unread or idle. */
export function threadMarkState(status: ThreadStatus, unread: boolean): ThreadState {
  switch (status) {
    case "running":
    case "starting":
      return "running";
    case "awaiting":
      return "awaiting";
    case "failed":
      return "failed";
    default:
      return unread ? "unread" : "idle";
  }
}

/** Whether `status` turned awaiting or failed while shown (not when first shown): the row flashes (UI-169). */
export function useFresh(status: ThreadStatus): boolean {
  const previous = useRef(status);
  const [fresh, setFresh] = useState(false);
  useEffect(() => {
    if (previous.current !== status) {
      previous.current = status;
      setFresh(status === "awaiting" || status === "failed");
    }
  }, [status]);
  return fresh;
}
