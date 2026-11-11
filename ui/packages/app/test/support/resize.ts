/**
 * A `ResizeObserver` the test drives (happy-dom lays nothing out): {@link installResizeObserver} replaces the global
 * one until the returned function restores it; {@link resize} reports a new border-box width of an element to the
 * observers watching it.
 */

import { act } from "@testing-library/react";

class FakeResizeObserver {
  static readonly live = new Set<FakeResizeObserver>();
  readonly targets = new Set<Element>();
  readonly #callback: ResizeObserverCallback;

  constructor(callback: ResizeObserverCallback) {
    this.#callback = callback;
    FakeResizeObserver.live.add(this);
  }

  observe(target: Element): void {
    this.targets.add(target);
  }

  unobserve(target: Element): void {
    this.targets.delete(target);
  }

  disconnect(): void {
    this.targets.clear();
    FakeResizeObserver.live.delete(this);
  }

  report(target: Element, width: number): void {
    const entry = {
      target,
      contentRect: { width, height: 0 },
      borderBoxSize: [{ inlineSize: width, blockSize: 0 }],
    } as unknown as ResizeObserverEntry;
    this.#callback([entry], this as unknown as ResizeObserver);
  }
}

export function installResizeObserver(): () => void {
  const original = globalThis.ResizeObserver;
  globalThis.ResizeObserver = FakeResizeObserver as unknown as typeof ResizeObserver;
  return () => {
    globalThis.ResizeObserver = original;
    FakeResizeObserver.live.clear();
  };
}

/** Tells the observers of `element` that it is `width` px wide (border box). */
export function resize(element: Element, width: number): void {
  act(() => {
    for (const observer of [...FakeResizeObserver.live]) {
      if (observer.targets.has(element)) {
        observer.report(element, width);
      }
    }
  });
}
