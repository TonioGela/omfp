/**
 * {@link Transport} over the standard `WebSocket` API (the browser's, or Bun's global). A browser sends the page's
 * `Origin`, which the daemon checks (PRT-003); Bun sends none, which the daemon accepts as a non-browser client.
 */

import { type Connector, describeClosed, type Transport, TransportError, type TransportListener } from "./transport.ts";

/** A {@link Connector} opening a WebSocket to `url` (`ws://host:port/api/ws`) for every attempt. */
export function webSocketConnector(url: string): Connector {
  return (listener) => new WebSocketTransport(url, listener);
}

class WebSocketTransport implements Transport {
  readonly #socket: WebSocket | undefined;

  constructor(url: string, listener: TransportListener) {
    let socket: WebSocket;
    try {
      socket = new WebSocket(url);
    } catch (error) {
      // Callbacks must not run inside the connector call.
      const message = error instanceof Error ? error.message : String(error);
      queueMicrotask(() => listener.failed(`cannot connect: ${message}`));
      this.#socket = undefined;
      return;
    }
    this.#socket = socket;
    let opened = false;
    socket.binaryType = "arraybuffer";
    socket.onopen = () => {
      opened = true;
      listener.opened();
    };
    socket.onmessage = (event: MessageEvent<unknown>) => {
      // Binary frames carry terminal data (M6).
      if (typeof event.data === "string") {
        listener.received(event.data);
      }
    };
    socket.onclose = (event: CloseEvent) => {
      const closed = { code: event.code, reason: event.reason };
      if (opened) {
        listener.closed(closed);
      } else {
        listener.failed(`cannot connect: ${describeClosed(closed)}`);
      }
    };
    // An error is always followed by a close event, which carries the details.
    socket.onerror = null;
  }

  send(text: string): void {
    const socket = this.#socket;
    if (socket === undefined || socket.readyState !== WebSocket.OPEN) {
      throw new TransportError("the WebSocket is not open");
    }
    socket.send(text);
  }

  close(): void {
    const socket = this.#socket;
    if (socket === undefined) {
      return;
    }
    socket.onopen = null;
    socket.onmessage = null;
    socket.onclose = null;
    socket.onerror = null;
    socket.close(1000);
  }
}
