/** Whether the window's primary pointer is a finger (UI-421, UI-423): followed live, as a tablet docks a keyboard. */

import { useSyncExternalStore } from "react";

const COARSE_POINTER = "(pointer: coarse)";

function coarse(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia(COARSE_POINTER).matches;
}

function subscribe(changed: () => void): () => void {
  if (typeof window.matchMedia !== "function") {
    return () => undefined;
  }
  const query = window.matchMedia(COARSE_POINTER);
  query.addEventListener("change", changed);
  return () => query.removeEventListener("change", changed);
}

/** `(pointer: coarse)`: a touch screen, whose on-screen keyboard has no modifiers and no hardware Enter. */
export function useCoarsePointer(): boolean {
  return useSyncExternalStore(subscribe, coarse, () => false);
}
