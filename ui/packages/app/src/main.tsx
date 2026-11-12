/**
 * The web app's entry: the access token or a one-time ticket from the address bar (UI-002, UI-255), one client over
 * the page's own origin, the host with per-frame snapshots (UI-012), and the frame; or, at `/kit`, the component
 * gallery (UI-173). Styles: the kit's tokens and components, then the app's and the widgets' own (all token-driven,
 * UI-091); the daemon's user theme comes last (`/theme.css`, see `theme/theme.ts`).
 */

import "@omfp/kit/tokens.css";
import "@omfp/kit/kit.css";
import "./styles.css";
import "./widgets/chat/chat.css";

import { type Client, type Credential, connect, webSocketConnector } from "@omfp/client";
import { INDEX_TOPIC } from "@omfp/protocol";
import { type ReactNode, useEffect, useState, useSyncExternalStore } from "react";
import { createRoot } from "react-dom/client";
import { TokenScreen } from "./frame/dialogs.tsx";
import { Frame } from "./frame/frame.tsx";
import { browserNotifications } from "./frame/notifications.ts";
import { Gallery } from "./gallery/gallery.tsx";
import { isGalleryPath } from "./gallery/params.ts";
import { createHost, type OmfpHost, OmfpProvider, SELECTED_THREAD } from "./sdk/index.ts";
import { fragmentLocal, rejection, TOKEN_KEY, takeCredential, takeThread } from "./vm/token.ts";

/** The client name recorded as provenance (PRT-033). */
const CLIENT_NAME = "omfp/web";

/** `localStorage`, or none where the page may not use it (private mode). */
function pageStorage(): Storage | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}

/** The daemon's WebSocket on the origin that served the page. */
function socketUrl(): string {
  const scheme = window.location.protocol === "https:" ? "wss" : "ws";
  return `${scheme}://${window.location.host}/api/ws`;
}

function Root(): ReactNode {
  const [credential, setCredential] = useState(() =>
    takeCredential({ location: window.location, history: window.history, storage: pageStorage() }),
  );
  // Bumped on every submitted token, so the same token can be tried again.
  const [attempt, setAttempt] = useState(0);
  const submit = (value: string) => {
    pageStorage()?.setItem(TOKEN_KEY, value);
    setCredential({ token: value });
    setAttempt((count) => count + 1);
  };
  if (credential === undefined) {
    return <TokenScreen rejected={undefined} onSubmit={submit} />;
  }
  return <Connection key={attempt} credential={credential} onToken={submit} />;
}

/** One connection with `credential` while mounted; the token screen again when the daemon refuses it. */
function Connection({ credential, onToken }: { credential: Credential; onToken: (token: string) => void }): ReactNode {
  const [{ host, client }] = useState(() => openHost(credential));
  const [notifications] = useState(browserNotifications);
  useEffect(() => {
    host.frames.start();
    const release = host.topics.retain(INDEX_TOPIC);
    return () => {
      release();
      host.frames.stop();
      client.close();
    };
  }, [host, client]);
  const refusal = useSyncExternalStore(host.frames.subscribe, () => {
    const { connection } = host.frames.getSnapshot();
    return connection.kind === "unauthenticated" ? connection.message : undefined;
  });
  if (refusal !== undefined) {
    return <TokenScreen rejected={rejection(credential, refusal)} onSubmit={onToken} />;
  }
  return (
    <OmfpProvider host={host}>
      <Frame {...(notifications === undefined ? {} : { notifications })} />
    </OmfpProvider>
  );
}

function openHost(credential: Credential): { host: OmfpHost; client: Client } {
  const storage = pageStorage();
  const client = connect({
    ...credential,
    connector: webSocketConnector(socketUrl()),
    client: CLIENT_NAME,
    // A ticket's token is kept for the next page load, as a `#token=` link's is (UI-255).
    onToken: (token) => storage?.setItem(TOKEN_KEY, token),
  });
  // `#local=1`: this window runs on the daemon's machine (`omfp desktop`, UI-188).
  const local = fragmentLocal(window.location.hash);
  const host = createHost(client, storage === undefined ? { local } : { storage, local });
  // `#thread=<id>`: the window opens on that thread (the tray's waiting threads, UI-218).
  const thread = takeThread({ location: window.location, history: window.history });
  if (thread !== undefined) {
    host.window.set(SELECTED_THREAD, thread);
  }
  return { host, client };
}

const container = document.getElementById("root");
if (container === null) {
  throw new Error("index.html has no #root");
}
// `/kit`: the component gallery (UI-173), without a connection.
createRoot(container).render(
  isGalleryPath(window.location.pathname) ? <Gallery search={window.location.search} /> : <Root />,
);
