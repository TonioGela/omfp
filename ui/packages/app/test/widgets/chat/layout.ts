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
