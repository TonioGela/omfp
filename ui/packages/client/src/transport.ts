/**
 * One connection to the daemon's `/api/ws`, as text frames in and out (UI-001). The session talks to a {@link Transport}
 * it gets from a {@link Connector}, so tests substitute an in-memory one and the real client uses a WebSocket.
 */

/** The connection could not be opened or used. */
export class TransportError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TransportError";
  }
}

/** How a connection ended. */
export interface Closed {
  /** WebSocket close code, when the peer sent one (4401: unauthenticated, 4426: version mismatch, 1001: daemon
   * shutting down). */
  readonly code: number | undefined;
  readonly reason: string;
}

/** Human-readable form of {@link Closed}, used as the reconnect reason. */
export function describeClosed(closed: Closed): string {
  if (closed.code !== undefined) {
    return closed.reason === ""
      ? `connection closed (${closed.code})`
      : `connection closed (${closed.code}): ${closed.reason}`;
  }
  return closed.reason === "" ? "connection lost" : `connection lost: ${closed.reason}`;
}

/** An open (or opening) connection. */
export interface Transport {
  /** Sends one text frame; throws {@link TransportError} when the frame could not be handed to the connection. */
  send(text: string): void;
  /** Closes the connection normally (1000). No listener callback follows. */
  close(): void;
}

/**
 * What a transport reports. Exactly one of `opened` or `failed` comes first; after `opened`, any number of `received`
 * and at most one `closed`. Callbacks never run synchronously inside the {@link Connector} call that created the
 * transport.
 */
export interface TransportListener {
  /** The connection is open: frames may be sent. */
  opened(): void;
  /** One text frame from the daemon. */
  received(text: string): void;
  /** The open connection ended. */
  closed(closed: Closed): void;
  /** The connection could not be opened. */
  failed(error: string): void;
}

/** Opens a connection reporting to `listener`; called again for every reconnect attempt. */
export type Connector = (listener: TransportListener) => Transport;
