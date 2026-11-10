/**
 * Test host for app and widget tests: a fake protocol client over the real client `Model`, manual animation frames,
 * and helpers to feed index and thread state. Tests render widgets inside {@link renderWithHost}.
 */

import { type CallMethod, Model } from "@omfp/client";
import { replay, type ThreadEvent } from "@omfp/core";
import {
  type EventFrame,
  INDEX_TOPIC,
  type IndexEvent,
  type IndexSnapshot,
  type ParamsOf,
  type ResultOf,
  type Topic,
  threadTopic,
} from "@omfp/protocol";
import { act, type RenderResult, render } from "@testing-library/react";
import type { ReactNode } from "react";
import { createHost, type OmfpHost, OmfpProvider, type SdkClient } from "../../src/sdk/index.ts";

/** One `call` the code under test made; answer it with {@link CallRecord.resolve} or {@link CallRecord.reject}. */
export interface CallRecord {
  readonly method: CallMethod;
  readonly params: unknown;
  resolve(result: unknown): void;
  reject(error: unknown): void;
}

/** A client whose model the test drives directly. */
export class FakeClient implements SdkClient {
  readonly model = new Model();
  readonly calls: CallRecord[] = [];
  readonly subscribed: Topic[] = [];
  readonly unsubscribed: Topic[] = [];
  /** Answers for calls by method, applied as soon as the call is made (else the call stays pending). */
  readonly answers = new Map<CallMethod, (params: unknown) => unknown>();
  #waiters: Array<() => void> = [];
  #positions = new Map<Topic, number>();

  changes(): { next(): Promise<void> } {
    let seen = this.model.revision;
    return {
      next: async () => {
        while (this.model.revision === seen) {
          const { promise, resolve } = Promise.withResolvers<void>();
          this.#waiters.push(resolve);
          await promise;
        }
        seen = this.model.revision;
      },
    };
  }

  takeDirty() {
    return this.model.takeDirty();
  }

  call<M extends CallMethod>(method: M, params: ParamsOf<M>): Promise<ResultOf<M>> {
    const { promise, resolve, reject } = Promise.withResolvers<ResultOf<M>>();
    this.calls.push({ method, params, resolve: resolve as (result: unknown) => void, reject });
    const answer = this.answers.get(method);
    if (answer !== undefined) {
      resolve(answer(params) as ResultOf<M>);
    }
    return promise;
  }

  subscribe(topic: Topic): void {
    this.subscribed.push(topic);
    this.model.track(topic);
    this.notify();
  }

  unsubscribe(topic: Topic): void {
    this.unsubscribed.push(topic);
    this.model.untrack(topic);
    this.notify();
  }

  /** Calls to `method` so far. */
  callsTo(method: CallMethod): CallRecord[] {
    return this.calls.filter((call) => call.method === method);
  }

  /** Marks the connection ready (`system.hello` answered). */
  connected(): void {
    this.model.setConnection({
      kind: "connected",
      hello: { protocol: 1, daemon: "0.1.0", omp: "18.18.6", warnings: [] },
    });
    this.notify();
  }

  /** Answers the index subscription with `snapshot`. */
  index(snapshot: Partial<IndexSnapshot>): void {
    this.model.track(INDEX_TOPIC);
    this.model.applySubscribed(INDEX_TOPIC, {
      mode: "snapshot",
      epoch: "e1",
      seq: 0,
      state: {
        repositories: [],
        workspaces: [],
        threads: [],
        theme: { css: null },
        settings: { ui: {}, keymap: [], error: null },
        ...snapshot,
      },
    });
    this.#positions.set(INDEX_TOPIC, 0);
    this.notify();
  }

  /** Applies index events after {@link FakeClient.index}. */
  indexEvents(...events: IndexEvent[]): void {
    for (const event of events) {
      this.#apply(INDEX_TOPIC, event);
    }
    this.notify();
  }

  /** Answers `thread/<id>` with the state after `events` (the thread must be followed, e.g. by `useThread`). */
  thread(threadId: string, events: readonly ThreadEvent[]): void {
    const topic = threadTopic(threadId);
    this.model.track(topic);
    this.model.applySubscribed(topic, { mode: "snapshot", epoch: "e1", seq: 0, state: replay([...events]) });
    this.#positions.set(topic, 0);
    this.notify();
  }

  /** Applies live thread events after {@link FakeClient.thread}. */
  threadEvents(threadId: string, ...events: ThreadEvent[]): void {
    for (const event of events) {
      this.#apply(threadTopic(threadId), event);
    }
    this.notify();
  }

  /** Wakes the frame store after a direct model change. */
  notify(): void {
    const waiters = this.#waiters;
    this.#waiters = [];
    for (const wake of waiters) {
      wake();
    }
  }

  #apply(topic: Topic, event: EventFrame["event"]): void {
    const seq = (this.#positions.get(topic) ?? 0) + 1;
    this.#positions.set(topic, seq);
    this.model.applyEvent({ type: "event", topic, epoch: "e1", seq, event });
  }
}

/** Animation frames run by the test. */
export class ManualFrames {
  #pending: Array<() => void> = [];

  readonly schedule = (commit: () => void): void => {
    this.#pending.push(commit);
  };

  get pending(): number {
    return this.#pending.length;
  }

  /** Runs the frames requested so far. */
  run(): void {
    const pending = this.#pending;
    this.#pending = [];
    for (const commit of pending) {
      commit();
    }
  }
}

export interface TestHost {
  readonly client: FakeClient;
  readonly host: OmfpHost;
  /** Commits the model to the snapshot now (as an animation frame would) inside `act`. */
  flush(): void;
}

/** A host over a {@link FakeClient}; frames commit only on {@link TestHost.flush}. */
export function testHost(client = new FakeClient()): TestHost {
  const host = createHost(client, { schedule: () => {} });
  return {
    client,
    host,
    flush: () => act(() => host.frames.commit()),
  };
}

/** Renders `ui` inside the test host's provider. */
export function renderWithHost(ui: ReactNode, test: TestHost = testHost()): TestHost & { view: RenderResult } {
  const view = render(<OmfpProvider host={test.host}>{ui}</OmfpProvider>);
  return { ...test, view };
}
