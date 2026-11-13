/** The frame's status surfaces: connection banner (UI-003), the toasts of failed actions, actions that worked, the
 * running login and opened URLs, and the live region (UI-092). */

import { Button, LiveRegion, Toast, ToastRegion } from "@omfp/kit";
import { type ReactNode, useEffect, useMemo, useSyncExternalStore } from "react";
import {
  ERRORS,
  needsLogin,
  requestLogin,
  SELECTED_THREAD,
  SUCCESSES,
  useAnnounce,
  useConnection,
  useHost,
  useThreads,
  useWindowState,
} from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { announcement, banner } from "../vm/connection.ts";
import { LOGIN_RUN, LoginProgressToast } from "./login.tsx";
import { OPENED_URLS, OpenedUrlToast } from "./opened-urls.tsx";

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

/**
 * The frame's one toast stack (bottom right): errors of failed actions as error toasts (UI-160), each staying until
 * dismissed — with Log in when omp says it needs one (UI-387) —, actions that worked as success toasts going by
 * themselves (UI-385), the running login (UI-383), then the URLs omp opened (UI-346).
 */
export function Toasts(): ReactNode {
  const host = useHost();
  const [errors, setErrors] = useWindowState(ERRORS);
  const [successes, setSuccesses] = useWindowState(SUCCESSES);
  const [login] = useWindowState(LOGIN_RUN);
  const [opened, setOpened] = useWindowState(OPENED_URLS);
  if (errors.length === 0 && successes.length === 0 && login === undefined && opened.length === 0) {
    return null;
  }
  return (
    <ToastRegion data-testid="toasts">
      {errors.map((error, index) => {
        const dismiss = () => setErrors((all) => all.filter((_, at) => at !== index));
        return (
          <Toast
            // biome-ignore lint/suspicious/noArrayIndexKey: the same message may repeat; its place is its identity
            key={index}
            tone="error"
            className="omfp-error-toast"
            data-testid="error"
            dismissLabel={strings.shell.frame.dismiss}
            onDismiss={dismiss}
            action={
              needsLogin(error) && (
                <Button
                  size="sm"
                  variant="primary"
                  onPress={() => {
                    dismiss();
                    requestLogin(host);
                  }}
                  data-testid="error-login"
                >
                  {strings.shell.login.logIn}
                </Button>
              )
            }
          >
            {error}
          </Toast>
        );
      })}
      {successes.map((success) => (
        <Toast
          key={success.key}
          tone="success"
          data-testid="success"
          dismissLabel={strings.shell.frame.dismiss}
          onDismiss={() => setSuccesses((all) => all.filter((other) => other.key !== success.key))}
        >
          {success.text}
        </Toast>
      ))}
      <LoginProgressToast />
      {opened.map((item) => (
        <OpenedUrlToast
          key={item.key}
          opened={item}
          onDismiss={() => setOpened((all) => all.filter((other) => other.key !== item.key))}
        />
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
