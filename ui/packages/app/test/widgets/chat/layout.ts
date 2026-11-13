/**
 * A fake layout for happy-dom, which lays nothing out: the transcript gets a viewport (`clientHeight`, read by the
 * virtualizer) and every transcript item a height (`offsetHeight`, `getBoundingClientRect`), so the virtualizer renders a window of items as in a browser. Call the returned function to restore.
 */

const VIEWPORT = { width: 800, height: 600 };

export function fakeLayout(itemHeight = 100): () => void {
  const proto = HTMLElement.prototype;
  const saved = {
    clientHeight: Object.getOwnPropertyDescriptor(proto, "clientHeight"),
    clientWidth: Object.getOwnPropertyDescriptor(proto, "clientWidth"),
    offsetHeight: Object.getOwnPropertyDescriptor(proto, "offsetHeight"),
    rect: Object.getOwnPropertyDescriptor(Element.prototype, "getBoundingClientRect"),
  };
  const isItem = (element: Element) => element.getAttribute("data-testid") === "transcript-item";
  const isTranscript = (element: Element) => element.getAttribute("data-testid") === "transcript";
  Object.defineProperty(proto, "clientHeight", {
    configurable: true,
    get(this: Element) {
      return isTranscript(this) ? VIEWPORT.height : 0;
    },
  });
  Object.defineProperty(proto, "clientWidth", {
    configurable: true,
    get(this: Element) {
      return isTranscript(this) ? VIEWPORT.width : 0;
    },
  });
  Object.defineProperty(proto, "offsetHeight", {
    configurable: true,
    get(this: Element) {
      return isItem(this) ? itemHeight : 0;
    },
  });
  Object.defineProperty(Element.prototype, "getBoundingClientRect", {
    configurable: true,
    value(this: Element) {
      const height = isItem(this) ? itemHeight : 0;
      return new DOMRect(0, 0, VIEWPORT.width, height);
    },
  });
  return () => {
    for (const [name, descriptor] of [
      ["clientHeight", saved.clientHeight],
      ["clientWidth", saved.clientWidth],
      ["offsetHeight", saved.offsetHeight],
    ] as const) {
      if (descriptor === undefined) {
        Reflect.deleteProperty(proto, name);
      } else {
        Object.defineProperty(proto, name, descriptor);
      }
    }
    if (saved.rect === undefined) {
      Reflect.deleteProperty(Element.prototype, "getBoundingClientRect");
    } else {
      Object.defineProperty(Element.prototype, "getBoundingClientRect", saved.rect);
    }
  };
}

/** Where the list starts in the transcript's content (its padding). */
export const LIST_TOP = 16;

/**
 * Over {@link fakeLayout}, a layout where positions follow scrolling, for the transcript's paging (UI-370, UI-371):
 * item `index` is `heights(index)` tall at the virtualizer's `translateY`, below {@link LIST_TOP}; the transcript's
 * viewport is at the top of the page, its content as tall as the list plus its padding, and its `scrollTop` clamped
 * to that content as in a browser (setting it fires no `scroll` event). Call the returned function to restore.
 */
export function pagedLayout(heights: (index: number) => number): () => void {
  const proto = HTMLElement.prototype;
  const saved = {
    offsetHeight: Object.getOwnPropertyDescriptor(proto, "offsetHeight"),
    scrollTop: Object.getOwnPropertyDescriptor(proto, "scrollTop"),
    scrollHeight: Object.getOwnPropertyDescriptor(proto, "scrollHeight"),
    rect: Object.getOwnPropertyDescriptor(Element.prototype, "getBoundingClientRect"),
  };
  const elementScrollTop = Object.getOwnPropertyDescriptor(Element.prototype, "scrollTop");
  const positions = new WeakMap<Element, number>();
  const isTranscript = (element: Element) => element.getAttribute("data-testid") === "transcript";
  const index = (element: Element) => {
    const value = element.getAttribute("data-index");
    return element.getAttribute("data-testid") === "transcript-item" && value !== null ? Number(value) : undefined;
  };
  const listHeight = (transcript: Element) =>
    Number.parseFloat(transcript.querySelector<HTMLElement>(".omfp-transcript__list")?.style.height ?? "0");
  const contentHeight = (transcript: Element) => listHeight(transcript) + 2 * LIST_TOP;
  const scrolled = (element: Element) => {
    const transcript = element.closest("[data-testid=transcript]");
    return transcript === null ? 0 : (positions.get(transcript) ?? 0);
  };
  Object.defineProperty(proto, "offsetHeight", {
    configurable: true,
    get(this: Element) {
      const at = index(this);
      return at === undefined ? 0 : heights(at);
    },
  });
  Object.defineProperty(proto, "scrollHeight", {
    configurable: true,
    get(this: Element) {
      return isTranscript(this) ? contentHeight(this) : 0;
    },
  });
  Object.defineProperty(proto, "scrollTop", {
    configurable: true,
    get(this: Element) {
      return isTranscript(this) ? (positions.get(this) ?? 0) : (elementScrollTop?.get?.call(this) ?? 0);
    },
    set(this: Element, value: number) {
      if (isTranscript(this)) {
        const height = (this as HTMLElement).clientHeight;
        positions.set(this, Math.min(Math.max(0, contentHeight(this) - height), Math.max(0, value)));
      } else {
        elementScrollTop?.set?.call(this, value);
      }
    },
  });
  Object.defineProperty(Element.prototype, "getBoundingClientRect", {
    configurable: true,
    value(this: HTMLElement) {
      const width = 800;
      if (isTranscript(this)) {
        return new DOMRect(0, 0, width, this.clientHeight);
      }
      if (this.classList.contains("omfp-transcript__list")) {
        return new DOMRect(
          0,
          LIST_TOP - scrolled(this),
          width,
          listHeight(this.closest("[data-testid=transcript]") ?? this),
        );
      }
      const at = index(this);
      if (at === undefined) {
        return new DOMRect(0, 0, 0, 0);
      }
      const start = Number.parseFloat(/translateY\(([-\d.]+)px\)/.exec(this.style.transform)?.[1] ?? "0");
      return new DOMRect(0, LIST_TOP + start - scrolled(this), width, heights(at));
    },
  });
  return () => {
    for (const [name, descriptor] of [
      ["offsetHeight", saved.offsetHeight],
      ["scrollTop", saved.scrollTop],
      ["scrollHeight", saved.scrollHeight],
    ] as const) {
      if (descriptor === undefined) {
        Reflect.deleteProperty(proto, name);
      } else {
        Object.defineProperty(proto, name, descriptor);
      }
    }
    if (saved.rect === undefined) {
      Reflect.deleteProperty(Element.prototype, "getBoundingClientRect");
    } else {
      Object.defineProperty(Element.prototype, "getBoundingClientRect", saved.rect);
    }
  };
}
