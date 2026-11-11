/** The frame's status surfaces: connection banner (UI-003), errors of failed actions, and the live region (UI-092). */

import { LiveRegion, Toast, ToastRegion } from "@omfp/kit";
import { type ReactNode, useEffect, useMemo, useSyncExternalStore } from "react";
import {
  ERRORS,
  SELECTED_THREAD,
  useAnnounce,
  useConnection,
  useHost,
  useThreads,
  useWindowState,
} from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { announcement, banner } from "../vm/connection.ts";

/** Non-blocking: the window stays usable (and keeps its drafts) while the client reconnects. */
export function ConnectionBanner(): ReactNode {
  const shown = banner(useConnection());
  if (shown === undefined) {
    return null;
  }
  return (
    <div className="omfp-banner" data-testid="connection-banner" data-tone={shown.tone}>
      {shown.text}
    </div>
  );
}

/** Errors of failed actions as error toasts (UI-160), each staying until dismissed. */
export function Errors(): ReactNode {
  const [errors, setErrors] = useWindowState(ERRORS);
  if (errors.length === 0) {
    return null;
  }
  return (
    <ToastRegion data-testid="errors">
      {errors.map((error, index) => (
        <Toast
          // biome-ignore lint/suspicious/noArrayIndexKey: the same message may repeat; its place is its identity
          key={index}
          tone="error"
          data-testid="error"
          dismissLabel={strings.shell.frame.dismiss}
          onDismiss={() => setErrors((all) => all.filter((_, at) => at !== index))}
        >
          {error}
        </Toast>
      ))}
    </ToastRegion>
  );
}

/** Announces the status line (connection, selected thread's state, threads awaiting input) whenever it changes, and
 * renders the live region every widget speaks through (`useAnnounce`). */
export function StatusLiveRegion(): ReactNode {
  const { announcer } = useHost();
  const connection = useConnection();
  const threads = useThreads();
  const [selected] = useWindowState(SELECTED_THREAD);
  const announce = useAnnounce();
  const status = useMemo(
    () => announcement(connection, [...threads.values()], selected),
    [connection, threads, selected],
  );
  useEffect(() => {
    if (status !== "") {
      announce(status);
    }
  }, [status, announce]);
  const latest = useSyncExternalStore(announcer.subscribe, announcer.getSnapshot);
  return <LiveRegion message={latest.message} data-testid="live-region" />;
}
