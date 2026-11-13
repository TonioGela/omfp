/**
 * The first run (UI-500..UI-504): on a fresh install omp has no model it can use, so every thread start fails until
 * the user logs in. The window asks the daemon how many models omp can use (`auth.status`, PRT-140) and follows the
 * count on the `index` topic (PRT-141); with none and no thread selected, the main area welcomes the user and offers
 * the login instead of a view of empty panels. With usable models, or while the count is unknown, nothing changes.
 */

import { Button, Wordmark } from "@omfp/kit";
import { type ReactNode, useEffect } from "react";
import { requestLogin, useConnection, useHost, useSnapshot, useWindowState } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { LOGIN_RUN } from "./login.tsx";

const text = strings.shell.welcome;

/**
 * Whether the main area shows the welcome (UI-500): omp has no usable model and no thread is selected. While the
 * index does not know the count, asks `auth.status` once per connection (UI-502); the daemon publishes its answer on
 * the index, and a failed check changes nothing.
 */
export function useWelcome(selectedThread: string | undefined): boolean {
  const host = useHost();
  const connection = useConnection();
  const models = useSnapshot((snapshot) => snapshot.index.models);
  const connected = connection.kind === "connected" ? connection.hello.connection : undefined;
  const unknown = models === null;
  useEffect(() => {
    if (connected === undefined || !unknown) {
      return;
    }
    host.client.call("auth.status", {}).catch(() => {
      // omp could not start (e.g. it is missing): the frame stays as it is, and a thread's start says why.
    });
  }, [host, connected, unknown]);
  return models === 0 && selectedThread === undefined;
}

/** The welcome (UI-500, UI-501): why nothing can start yet, and the login that fixes it. */
export function Welcome(): ReactNode {
  const host = useHost();
  const [run] = useWindowState(LOGIN_RUN);
  return (
    <section className="omfp-welcome" data-testid="welcome" aria-labelledby="omfp-welcome-title">
      <Wordmark className="omfp-welcome__mark" aria-hidden="true" />
      <h1 id="omfp-welcome-title" className="omfp-welcome__title">
        {text.title}
      </h1>
      <p className="omfp-welcome__why" data-testid="welcome-why">
        {text.why}
      </p>
      <Button
        variant="primary"
        isPending={run !== undefined}
        onPress={() => requestLogin(host)}
        data-testid="welcome-login"
      >
        {text.logIn}
      </Button>
      <p className="omfp-welcome__keys" data-testid="welcome-keys">
        {text.keys}
      </p>
    </section>
  );
}
