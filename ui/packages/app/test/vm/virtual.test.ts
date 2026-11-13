import { describe, expect, test } from "bun:test";
import {
  arrivalScroll,
  halfPageScrollTop,
  INITIAL_PIN,
  isScrollUpKey,
  keyScrollTop,
  mountScrollTop,
  movedUpByUser,
  navTarget,
  type PinState,
  pagerScroll,
  pinReducer,
  refocusAfterScroll,
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
    expect(navTarget("nav.next", undefined, 10)).toBe(9);
    expect(navTarget("nav.prev", 5, 10)).toBe(4);
    expect(navTarget("nav.next", 9, 10)).toBe(9);
    expect(navTarget("nav.prev", 0, 10)).toBe(0);
    expect(navTarget("nav.top", 5, 10)).toBe(0);
    expect(navTarget("nav.bottom", 0, 10)).toBe(9);
    expect(navTarget("nav.next", undefined, 0)).toBe(-1);
  });

  // spec: UI-370
  test("j/k page through an item taller than the viewport, then move; an item that fits moves at once", () => {
    // A 600 px viewport: steps of 240 px, the last one up to the edge plus 16 px.
    expect(pagerScroll(1, { top: -100, bottom: 1500 }, 600)).toBe(240);
    expect(pagerScroll(1, { top: -900, bottom: 700 }, 600)).toBe(116);
    // An item that fits moves at once, even with its end out of view.
    expect(pagerScroll(1, { top: 550, bottom: 650 }, 600)).toBeUndefined();
    expect(pagerScroll(1, { top: -1000, bottom: 600 }, 600)).toBeUndefined();
    expect(pagerScroll(1, { top: 100, bottom: 400 }, 600)).toBeUndefined();
    // Out of view below: not paged (the move goes on as before).
    expect(pagerScroll(1, { top: 700, bottom: 2000 }, 600)).toBeUndefined();
    expect(pagerScroll(-1, { top: -1000, bottom: 500 }, 600)).toBe(-240);
    expect(pagerScroll(-1, { top: -50, bottom: 1000 }, 600)).toBe(-66);
    expect(pagerScroll(-1, { top: 0, bottom: 1000 }, 600)).toBeUndefined();
    expect(pagerScroll(-1, { top: -2000, bottom: -100 }, 600)).toBeUndefined();
    // Arriving on a tall item: its top going down, its end going up; nothing when that edge shows; items that fit
    // scroll into view as before.
    expect(arrivalScroll(1, { top: 500, bottom: 2000 }, 600)).toBe(0);
    expect(arrivalScroll(1, { top: 650, bottom: 2000 }, 600)).toBe(650);
    expect(arrivalScroll(-1, { top: -2000, bottom: 100 }, 600)).toBe(0);
    expect(arrivalScroll(-1, { top: -2000, bottom: -40 }, 600)).toBe(-640);
    expect(arrivalScroll(1, { top: 550, bottom: 700 }, 600)).toBeUndefined();
  });

  // spec: UI-371
  test("half pages scroll within the content; the focus moves only when its item left the view", () => {
    expect(halfPageScrollTop(1, 1000, 600, 5000)).toBe(1300);
    expect(halfPageScrollTop(1, 4300, 600, 5000)).toBe(4400);
    expect(halfPageScrollTop(-1, 200, 600, 5000)).toBe(0);
    const items = [0, 1, 2, 3, 4].map((index) => ({ start: index * 400, end: index * 400 + 400 }));
    expect(refocusAfterScroll(1, items, 700, 600)).toBeUndefined();
    expect(refocusAfterScroll(1, items, 800, 600)).toBe(2);
    expect(refocusAfterScroll(4, items, 0, 600)).toBe(0);
    expect(refocusAfterScroll(undefined, items, 800, 600)).toBeUndefined();
  });
});
