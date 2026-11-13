/** A thread row's attention motion (UI-146, UI-169). */

import { useEffect, useRef, useState } from "react";
import type { ThreadStatus } from "../../vm/threads.ts";

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
