/** A `window.matchMedia` stub for the primary pointer (UI-421, UI-423). */

const COARSE = "(pointer: coarse)";

/**
 * `matchMedia` answering `(pointer: coarse)` from a switch the test flips; other queries never match. Installed on
 * construction, put back by {@link PointerMedia.restore}.
 */
export class PointerMedia {
  coarse: boolean;
  readonly listeners = new Set<() => void>();
  readonly #original = window.matchMedia;

  constructor(coarse: boolean) {
    this.coarse = coarse;
    window.matchMedia = this.matchMedia;
  }

  readonly matchMedia = (query: string): MediaQueryList => {
    const media = this;
    const list: Pick<MediaQueryList, "media" | "matches" | "addEventListener" | "removeEventListener"> = {
      media: query,
      get matches() {
        return query === COARSE && media.coarse;
      },
      addEventListener: (_type: string, listener: unknown) => media.listeners.add(listener as () => void),
      removeEventListener: (_type: string, listener: unknown) => media.listeners.delete(listener as () => void),
    };
    return list as MediaQueryList;
  };

  /** Switches the pointer and tells the listeners, as a tablet docking a keyboard and mouse would. */
  set(coarse: boolean): void {
    this.coarse = coarse;
    for (const listener of this.listeners) {
      listener();
    }
  }

  restore(): void {
    window.matchMedia = this.#original;
  }
}
