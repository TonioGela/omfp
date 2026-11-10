import { describe, expect, test } from "bun:test";
import {
  INITIAL_PIN,
  isScrollUpKey,
  keyScrollTop,
  mountScrollTop,
  navTarget,
  type PinState,
  pinReducer,
} from "../../src/vm/virtual.ts";

const scroll = (scrollTop: number, scrollHeight: number, clientHeight: number) =>
  ({ type: "scroll", scrollTop, scrollHeight, clientHeight }) as const;

describe("pinning", () => {
  // spec: UI-042
  test("scrolling up unpins and reaching the bottom pins again", () => {
    let state: PinState = pinReducer(INITIAL_PIN, scroll(4400, 5000, 600));
    expect(state.pinned).toBe(true);
    state = pinReducer(state, { type: "userScrollUp" });
    state = pinReducer(state, scroll(1200, 5000, 600));
    expect(state.pinned).toBe(false);
    state = pinReducer(state, scroll(5000 - 620, 5000, 600));
    expect(state.pinned).toBe(true);
  });

  // spec: UI-042
  test("layout shifts keep the view pinned but dragging away unpins it", () => {
    let state = pinReducer(INITIAL_PIN, scroll(1000, 4000, 600));
    expect(state.pinned).toBe(true);
    state = pinReducer(state, { type: "drag", active: true });
    state = pinReducer(state, scroll(1000, 4000, 600));
    expect(state.pinned).toBe(false);
    state = pinReducer(state, scroll(3400, 4000, 600));
    expect(state.pinned).toBe(true);
  });

  // spec: UI-042
  test("a remounted list returns to the bottom or to where the user scrolled", () => {
    expect(mountScrollTop(INITIAL_PIN)).toBeUndefined();
    let state = pinReducer(INITIAL_PIN, { type: "userScrollUp" });
    state = pinReducer(state, scroll(400, 5000, 600));
    expect(mountScrollTop(state)).toBe(400);
    state = pinReducer(state, scroll(4400, 5000, 600));
    expect(mountScrollTop(state)).toBeUndefined();
  });

  // spec: UI-042
  test("going to the end on purpose pins the view", () => {
    const up = pinReducer(INITIAL_PIN, { type: "userScrollUp" });
    expect(pinReducer(up, { type: "pin" }).pinned).toBe(true);
  });

  test("an event that changes nothing keeps the same state object", () => {
    expect(pinReducer(INITIAL_PIN, { type: "drag", active: false })).toBe(INITIAL_PIN);
  });
});

describe("keys", () => {
  // spec: UI-042, UI-095
  test("scroll-up keys unpin; paging keys scroll the transcript itself", () => {
    expect(["ArrowUp", "PageUp", "Home"].map(isScrollUpKey)).toEqual([true, true, true]);
    expect(["ArrowDown", "End", "k"].map(isScrollUpKey)).toEqual([false, false, false]);
    expect(keyScrollTop("PageUp", 1000, 600, 5000)).toBe(432);
    expect(keyScrollTop("PageDown", 4200, 600, 5000)).toBe(4400);
    expect(keyScrollTop("Home", 1000, 600, 5000)).toBe(0);
    expect(keyScrollTop("End", 0, 600, 5000)).toBe(4400);
    expect(keyScrollTop("a", 0, 600, 5000)).toBeUndefined();
  });

  // spec: UI-095
  test("navigation commands move between messages and stay in range", () => {
    expect(navTarget("nav.next", undefined, 10, 3)).toBe(9);
    expect(navTarget("nav.prev", 5, 10, 3)).toBe(4);
    expect(navTarget("nav.next", 9, 10, 3)).toBe(9);
    expect(navTarget("nav.top", 5, 10, 3)).toBe(0);
    expect(navTarget("nav.bottom", 0, 10, 3)).toBe(9);
    expect(navTarget("nav.halfDown", 8, 10, 3)).toBe(9);
    expect(navTarget("nav.halfUp", 2, 10, 3)).toBe(0);
    expect(navTarget("nav.next", undefined, 0, 3)).toBe(-1);
  });
});
