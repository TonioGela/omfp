/** Renders a conversation widget with a selected thread fed from a recorded fixture. */

import type { ThreadEvent } from "@omfp/core";
import type { ReactNode } from "react";
import { SELECTED_THREAD } from "../../../src/sdk/index.ts";
import type { Clock } from "../../../src/widgets/chat/state.ts";
import { ClockContext } from "../../../src/widgets/chat/state.ts";
import { renderWithHost, type TestHost, testHost } from "../../support/host.tsx";
import { summary } from "./fixtures.ts";

export const THREAD = "01T";

/** Renders `ui` with {@link THREAD} selected and its state after `events`. */
export function renderThread(ui: ReactNode, events: readonly ThreadEvent[], clock?: Clock) {
  const test: TestHost = testHost();
  test.client.connected();
  test.client.index({ threads: [summary(THREAD)] });
  test.host.window.set(SELECTED_THREAD, THREAD);
  const rendered = renderWithHost(clock === undefined ? ui : <ClockContext value={clock}>{ui}</ClockContext>, test);
  test.client.thread(THREAD, events);
  test.flush();
  return rendered;
}

/** A clock the test advances. */
export class ManualClock implements Clock {
  #now: number;
  #tickers = new Set<() => void>();

  constructor(now: number) {
    this.#now = now;
  }

  now(): number {
    return this.#now;
  }

  every(_ms: number, tick: () => void): () => void {
    this.#tickers.add(tick);
    return () => this.#tickers.delete(tick);
  }

  /** Moves time forward by `ms` and runs the tickers. */
  advance(ms: number): void {
    this.#now += ms;
    for (const tick of [...this.#tickers]) {
      tick();
    }
  }
}
