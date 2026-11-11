import { describe, expect, test } from "bun:test";
import {
  INITIAL_PIN,
  isScrollUpKey,
  keyScrollTop,
  mountScrollTop,
  movedUpByUser,
  navTarget,
  type PinState,
  pinReducer,
} from "../../src/vm/virtual.ts";

const scroll = (scrollTop: number, scrollHeight: number, clientHeight: number, previousScrollTop?: number) =>
  ({ type: "scroll", scrollTop, scrollHeight, clientHeight, previousScrollTop }) as const;

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
  test("layout shifts keep the view pinned", () => {
    // Content grew under a pinned view: the position stays, the bottom moved away.
    let state = pinReducer(INITIAL_PIN, scroll(1000, 4000, 600, 1000));
    expect(state.pinned).toBe(true);
    // Content shrank: the browser clamped the position to the new end.
    state = pinReducer(state, scroll(800, 1400, 600, 1000));
    expect(state.pinned).toBe(true);
  });

  // spec: UI-042
  test("a scroll up away from the bottom unpins without any input event (WebKitGTK's scrollbar sends none)", () => {
    const state = pinReducer(INITIAL_PIN, scroll(3000, 4000, 600, 3400));
    expect([state.pinned, mountScrollTop(state)]).toEqual([false, 3000]);
  });

  // spec: UI-042
  test("a scroll up of a few pixels unpins even within the threshold: smooth scrolling and touchpads step that small", () => {
    // WebKitGTK's first frame of a wheel notch while a reply streams: 5 px up from the end.
    let state = pinReducer(INITIAL_PIN, scroll(12502, 13106, 600, 12507));
    expect([state.pinned, mountScrollTop(state)]).toEqual([false, 12502]);
    // The next frames move further up; nothing pins it again.
    state = pinReducer(state, scroll(12490, 13126, 600, 12502));
    expect(state.pinned).toBe(false);
    // No move (a scroll event after a follow already saw the position): still unpinned within the threshold.
    expect(pinReducer(state, scroll(12490, 13106, 600, 12490)).pinned).toBe(false);
    // Moving down into the threshold pins again.
    expect(pinReducer(state, scroll(12500, 13120, 600, 12490)).pinned).toBe(true);
  });

  // spec: UI-042
  test("a scroll event that did not move the view changes nothing, even at the very end", () => {
    // A follow put the view at the end (12507) and WebKitGTK dispatches its scroll event at the next frame, after the
    // user's wheel unpinned the view: it must not pin it again before the wheel's smooth scrolling moves it.
    const wheeled = pinReducer(INITIAL_PIN, { type: "userScrollUp" });
    expect(pinReducer(wheeled, scroll(12507, 13107, 600, 12507)).pinned).toBe(false);
    // Clamped to the end of content that shrank: pinned again.
    expect(pinReducer(wheeled, scroll(12407, 13007, 600, 12507)).pinned).toBe(true);
    // The first frame of the wheel's smooth scroll, 2 px up from WebKitGTK's end (-1 below): stays unpinned.
    expect(pinReducer(wheeled, scroll(12454, 13055, 600, 12456)).pinned).toBe(false);
  });

  // spec: UI-042
  test("a move up is the user's unless the view sits at the very end", () => {
    const position = (scrollTop: number, scrollHeight: number) => ({ scrollTop, scrollHeight, clientHeight: 600 });
    expect(movedUpByUser(5000, position(4995, 5700))).toBe(true);
    // WebKitGTK's rounded positions: -1 below at the end, 1 after a smooth scroll's first 2 px frame up.
    expect(movedUpByUser(12456, position(12454, 13055))).toBe(true);
    // Clamped to the end of content that shrank (WebKitGTK may report -1), or a sub-pixel move.
    expect(movedUpByUser(5000, position(4900, 5500))).toBe(false);
    expect(movedUpByUser(5000, position(4900, 5499))).toBe(false);
    expect(movedUpByUser(5000, position(4999.5, 5700))).toBe(false);
    // Down (content grew below a followed view, or the user scrolled down), or nothing to compare with.
    expect(movedUpByUser(5000, position(5100, 6000))).toBe(false);
    expect(movedUpByUser(undefined, position(1000, 6000))).toBe(false);
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
    expect(pinReducer(INITIAL_PIN, { type: "pin" })).toBe(INITIAL_PIN);
  });

  // spec: UI-191
  test("a layout scroll while pinned keeps the same state object, so streaming does not render the list again", () => {
    const pinned = pinReducer(INITIAL_PIN, scroll(4400, 5000, 600));
    expect(pinned).toBe(INITIAL_PIN);
    expect(pinReducer(pinned, scroll(1000, 5000, 600))).toBe(INITIAL_PIN);
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
