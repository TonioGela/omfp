/**
 * Transcript scrolling (UI-042): `@tanstack/react-virtual` keeps only the visible items in the DOM; this module decides
 * when the view follows the bottom, which keys the transcript scrolls itself (WebKitGTK ignores PageUp on a focused
 * container, S-11), and where `nav.*` commands go.
 *
 * Pinned: while streaming, growth keeps the view at the bottom. Only the user unpins it — wheel up, a scroll-up key,
 * or any move of the view up from where the last scroll or follow left it, unless the view sits at the very end
 * (shrinking content clamps it there). Layout never moves a pinned view up otherwise: the transcript turns off the
 * virtualizer's scroll adjustments and the browser's scroll anchoring while pinned. A move up unpins even within
 * {@link PIN_THRESHOLD_PX} of the bottom: smooth scrolling (WebKitGTK's wheel) and touchpads scroll a few pixels per
 * frame, and re-pinning on such a step lets the next streamed delta pull the view back to the bottom. Moving down to
 * within the threshold, or being clamped to the very end, pins it again. A scroll event that did not move the view
 * changes nothing: WebKitGTK dispatches the one a follow caused at the next frame, possibly after the user's wheel.
 */

/** The view counts as at the bottom within this distance. */
export const PIN_THRESHOLD_PX = 32;
/** The view is at the very end of the content less than this far from it (rounded positions: WebKitGTK reports -1 at
 * the end, and 1 after the first 2 px frame of a smooth scroll up). */
const END_PX = 1;

/** What one transcript knows about its scrolling, kept per thread so switching threads keeps the position. */
export interface PinState {
  /** Follow the bottom as content grows. */
  readonly pinned: boolean;
  /** Last scroll position while unpinned, restored when the list mounts again unpinned. Pinned, the list follows the
   * bottom and the position is not stored: a layout scroll while streaming then changes nothing (no render, UI-191). */
  readonly scrollTop: number;
}

export const INITIAL_PIN: PinState = { pinned: true, scrollTop: 0 };

/** Where the scroll container's view is. */
export interface ScrollPosition {
  readonly scrollTop: number;
  readonly scrollHeight: number;
  readonly clientHeight: number;
}

export type PinEvent =
  | ({
      readonly type: "scroll";
      /** The position before this scroll (the last scroll event's, or where following the bottom put it). */
      readonly previousScrollTop: number | undefined;
    } & ScrollPosition)
  /** Wheel up, a scroll-up key outside a form control, a navigation command moving up. */
  | { readonly type: "userScrollUp" }
  /** The user went to the end (`G`, `End`). */
  | { readonly type: "pin" };

/**
 * The view moved up from `previous` (where the last scroll or follow left it) to somewhere above the very end: the
 * user scrolled, since layout only moves a pinned view down or clamps it to the end. Whatever reports it first — the
 * scroll event, or a follow that runs before it (WebKitGTK dispatches scroll events at the next frame) — unpins.
 */
export function movedUpByUser(previous: number | undefined, position: ScrollPosition): boolean {
  const { scrollTop, scrollHeight, clientHeight } = position;
  return previous !== undefined && scrollTop < previous - 1 && scrollHeight - scrollTop - clientHeight >= END_PX;
}

/** The state after `event`; the same object when nothing changed. */
export function pinReducer(state: PinState, event: PinEvent): PinState {
  let next: PinState;
  switch (event.type) {
    case "scroll": {
      const below = event.scrollHeight - event.scrollTop - event.clientHeight;
      const previous = event.previousScrollTop;
      const movedDown = previous === undefined || event.scrollTop > previous + 1;
      const movedUp = previous !== undefined && event.scrollTop < previous - 1;
      const pinned =
        !movedUpByUser(previous, event) &&
        (state.pinned || (movedDown && below <= PIN_THRESHOLD_PX) || (movedUp && below < END_PX));
      next = { ...state, scrollTop: pinned ? state.scrollTop : event.scrollTop, pinned };
      break;
    }
    case "userScrollUp":
    case "pin":
      next = { ...state, pinned: event.type === "pin" };
      break;
  }
  return next.pinned === state.pinned && next.scrollTop === state.scrollTop ? state : next;
}

/** Where a freshly mounted list scrolls to: the stored position when the user had scrolled up, `undefined` for the
 * bottom. */
export function mountScrollTop(state: PinState): number | undefined {
  return state.pinned ? undefined : state.scrollTop;
}

/** Keys that scroll up and so unpin (unless typed into a form control, where they edit or move within it). */
export function isScrollUpKey(key: string): boolean {
  return key === "ArrowUp" || key === "PageUp" || key === "Home";
}

/** The scroll position after a paging key the transcript handles itself, `undefined` for other keys. */
export function keyScrollTop(
  key: string,
  scrollTop: number,
  clientHeight: number,
  scrollHeight: number,
): number | undefined {
  const page = Math.max(clientHeight - PIN_THRESHOLD_PX, clientHeight / 2);
  const bottom = Math.max(0, scrollHeight - clientHeight);
  switch (key) {
    case "PageUp":
      return Math.max(0, scrollTop - page);
    case "PageDown":
      return Math.min(bottom, scrollTop + page);
    case "Home":
      return 0;
    case "End":
      return bottom;
    default:
      return undefined;
  }
}

/** The transcript-scope navigation commands (UI-095). */
export type NavCommand = "nav.next" | "nav.prev" | "nav.top" | "nav.bottom" | "nav.halfDown" | "nav.halfUp";

/**
 * The item index a navigation command moves to from `current` (`undefined`: nothing focused yet, start from the
 * bottom) among `count` items, `halfPage` items per half page.
 */
export function navTarget(command: NavCommand, current: number | undefined, count: number, halfPage: number): number {
  if (count === 0) {
    return -1;
  }
  const last = count - 1;
  const from = current ?? last;
  const step = Math.max(1, halfPage);
  const clamp = (index: number) => Math.min(last, Math.max(0, index));
  switch (command) {
    case "nav.next":
      return current === undefined ? last : clamp(from + 1);
    case "nav.prev":
      return current === undefined ? last : clamp(from - 1);
    case "nav.top":
      return 0;
    case "nav.bottom":
      return last;
    case "nav.halfDown":
      return clamp(from + step);
    case "nav.halfUp":
      return clamp(from - step);
  }
}
