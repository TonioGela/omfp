/**
 * A scripted in-memory network and a manual timer for the session tests: the test plays the daemon, decides every
 * connection attempt, and fires timers by hand in virtual time, so reconnect behavior is checked without real time
 * (TST-003). The session runs synchronously; only call results and `done` are promises, settled by {@link flush}.
 */

import { OMP_VERSION } from "@omfp/core";
import type { HelloResult, MethodName, ParamsOf } from "@omfp/protocol";
import {
  type CallError,
  type CallMethod,
  type Client,
  type ConnectionStatus,
  type Connector,
  connect,
  type Timer,
  type Transport,
  TransportError,
  type TransportListener,
} from "../src/index.ts";

export const TOKEN = "secret";

/** A JSON object frame as the fake daemon sees it. */
export type Frame = Record<string, unknown>;

/** Lets pending promise callbacks run (a macrotask turn; no wall-clock wait). */
export function flush(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}

/** The client's side of a fake connection. */
class FakeTransport implements Transport {
  readonly sent: string[] = [];
  closedByClient = false;
  accepted = false;
  /** Sends that still succeed; `undefined`: unlimited. */
  sendBudget: number | undefined;

  send(text: string): void {
    if (!this.accepted || this.closedByClient) {
      throw new TransportError("closed");
    }
    if (this.sendBudget !== undefined) {
      if (this.sendBudget === 0) {
        throw new TransportError("send failed");
      }
      this.sendBudget -= 1;
    }
    this.sent.push(text);
  }

  close(): void {
    this.closedByClient = true;
  }
}

/** One connection attempt waiting for the test's decision. */
export class Attempt {
  readonly #listener: TransportListener;
  readonly #transport: FakeTransport;

  constructor(listener: TransportListener, transport: FakeTransport) {
    this.#listener = listener;
    this.#transport = transport;
  }

  accept(): ServerEnd {
    this.#transport.accepted = true;
    this.#listener.opened();
    return new ServerEnd(this.#listener, this.#transport);
  }

  refuse(message: string): void {
    this.#listener.failed(message);
  }
}

/** Connection attempts, in order. */
export class FakeNet {
  readonly #attempts: Attempt[] = [];
  /** Connection attempts so far. */
  attempts = 0;

  connector(): Connector {
    return (listener) => {
      const transport = new FakeTransport();
      this.#attempts.push(new Attempt(listener, transport));
      this.attempts += 1;
      return transport;
    };
  }

  takeAttempt(): Attempt {
    const attempt = this.#attempts.shift();
    if (attempt === undefined) {
      throw new Error("no pending connection attempt");
    }
    return attempt;
  }
}

/** The daemon's side of an accepted fake connection; frames reach the client synchronously. */
export class ServerEnd {
  readonly #listener: TransportListener;
  readonly #transport: FakeTransport;
  #read = 0;

  constructor(listener: TransportListener, transport: FakeTransport) {
    this.#listener = listener;
    this.#transport = transport;
  }

  /** The client's next `sends` sends succeed; every later one fails with a transport error. */
  failSendsAfter(sends: number): void {
    this.#transport.sendBudget = sends;
  }

  /** Frames the client sent since the last call. */
  frames(): Frame[] {
    const texts = this.#transport.sent.slice(this.#read);
    this.#read = this.#transport.sent.length;
    return texts.map((text) => JSON.parse(text) as Frame);
  }

  /** Whether the client closed its side. */
  get closedByClient(): boolean {
    return this.#transport.closedByClient;
  }

  send(frame: unknown): void {
    this.#listener.received(JSON.stringify(frame));
  }

  respond(id: unknown, result: unknown): void {
    this.send({ type: "response", id, ok: true, result });
  }

  fail(id: unknown, error: unknown): void {
    this.send({ type: "response", id, ok: false, error });
  }

  close(code?: number): void {
    this.#listener.closed({ code, reason: "" });
  }
}

/** A timer on virtual time: callbacks run when the test advances time or fires every pending one. */
export class ManualTimer implements Timer {
  /** Every delay requested so far, in order. */
  readonly requested: number[] = [];
  #now = 0;
  #pending: Array<{ at: number; fire: () => void }> = [];

  schedule(delayMs: number, fire: () => void): () => void {
    this.requested.push(delayMs);
    const entry = { at: this.#now + delayMs, fire };
    this.#pending.push(entry);
    return () => {
      this.#pending = this.#pending.filter((pending) => pending !== entry);
    };
  }

  /** Moves virtual time forward by `ms`, running the callbacks that fall due, in deadline order. */
  advance(ms: number): void {
    this.#now += ms;
    this.#runDue();
  }

  /** Runs every pending callback (time jumps to the latest deadline). */
  fire(): void {
    this.#now = Math.max(this.#now, ...this.#pending.map((pending) => pending.at));
    this.#runDue();
  }

  #runDue(): void {
    const due = this.#pending.filter((pending) => pending.at <= this.#now).sort((a, b) => a.at - b.at);
    this.#pending = this.#pending.filter((pending) => pending.at > this.#now);
    for (const pending of due) {
      pending.fire();
    }
  }
}

/** The outcome of a call, once settled. */
export type Settled = { ok: true; value: unknown } | { ok: false; error: CallError };

/** A call's outcome, filled in when its promise settles (after {@link flush}). */
export interface CallCell {
  settled: Settled | undefined;
}

export function hello(): HelloResult {
  return { protocol: 1, daemon: "0.1.0", omp: OMP_VERSION, warnings: [], widgetBase: "/widgets/test" };
}

/** A client on the fake network with a manual timer. */
export class Rig {
  readonly net = new FakeNet();
  readonly timer = new ManualTimer();
  readonly client: Client;
  #done = false;

  constructor() {
    this.client = connect({ connector: this.net.connector(), timer: this.timer, token: TOKEN, client: "test/l1" });
    void this.client.done.then(() => {
      this.#done = true;
    });
  }

  /** Whether the client stopped for good (read after {@link flush}). */
  get done(): boolean {
    return this.#done;
  }

  get status(): ConnectionStatus {
    return this.client.model.connection;
  }

  /** Accepts the pending connection attempt. */
  accept(): ServerEnd {
    return this.net.takeAttempt().accept();
  }

  /** Accepts the pending attempt and answers its hello; the connection and the frames sent after the hello. */
  connect(): [ServerEnd, Frame[]] {
    const server = this.accept();
    const [first, ...rest] = server.frames();
    if (first?.method !== "system.hello") {
      throw new Error(`expected hello first, got ${JSON.stringify(first)}`);
    }
    server.respond(first.id, hello());
    return [server, [...rest, ...server.frames()]];
  }

  /** Starts a call; its outcome appears in the returned cell once settled. */
  call<M extends CallMethod>(method: M, params: ParamsOf<M>): CallCell {
    return track(this.client.call(method, params));
  }

  /** Calls any method, session ones included, bypassing the static check. */
  callAny(method: MethodName, params: unknown): CallCell {
    const call = this.client.call.bind(this.client) as (method: MethodName, params: unknown) => Promise<unknown>;
    return track(call(method, params));
  }
}

function track(promise: Promise<unknown>): CallCell {
  const cell: CallCell = { settled: undefined };
  promise.then(
    (value) => {
      cell.settled = { ok: true, value };
    },
    (error: CallError) => {
      cell.settled = { ok: false, error };
    },
  );
  return cell;
}
