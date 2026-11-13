/**
 * A fake pdf.js for the component tests of `PdfView` (kit) and `omfp:file` (app): importing this module replaces
 * `pdfjs-dist` for the viewer's chunk. happy-dom has no canvas drawing nor worker, so the fake serves documents of
 * blank pages by URL and records what the viewer asks of it. `nearObserver` stands in for happy-dom's
 * `IntersectionObserver`, whose callbacks never run.
 */
import { mock } from "bun:test";

/** A document of `pages` pages, each `width` × `height` points; a string is the error loading it fails with. */
export type FakeDocument = { readonly pages: number; readonly width: number; readonly height: number } | string;

interface Viewport {
  readonly width: number;
  readonly height: number;
  readonly scale: number;
}

export const fakePdfjs = {
  /** The documents by URL; any other URL fails to load. */
  documents: new Map<string, FakeDocument>(),
  /** The options of every `getDocument`. */
  loads: [] as Record<string, unknown>[],
  /** The URLs of the loading tasks destroyed. */
  destroyed: [] as string[],
  /** Every page drawn: its number, the canvas size it was drawn at and the transform. */
  renders: [] as { page: number; canvasWidth: number; viewport: Viewport; transform: unknown }[],
  /** Every text layer: the page and the class of its container. */
  textLayers: [] as { page: number; container: string; viewport: Viewport }[],
  worker: { workerSrc: "" },
  reset(): void {
    this.documents.clear();
    this.loads.length = 0;
    this.destroyed.length = 0;
    this.renders.length = 0;
    this.textLayers.length = 0;
  },
};

const CSS_UNITS = 96 / 72;

function fakePage(number: number, document: Exclude<FakeDocument, string>) {
  return {
    pageNumber: number,
    getViewport: ({ scale }: { scale: number }): Viewport => ({
      width: document.width * scale,
      height: document.height * scale,
      scale,
    }),
    streamTextContent: () => ({ page: number }),
    render: ({
      canvas,
      viewport,
      transform,
    }: {
      canvas: HTMLCanvasElement;
      viewport: Viewport;
      transform?: unknown;
    }) => {
      fakePdfjs.renders.push({ page: number, canvasWidth: canvas.width, viewport, transform });
      return { promise: Promise.resolve(), cancel: () => undefined };
    },
  };
}

class FakeTextLayer {
  constructor({
    textContentSource,
    container,
    viewport,
  }: {
    textContentSource: { page: number };
    container: HTMLElement;
    viewport: Viewport;
  }) {
    fakePdfjs.textLayers.push({ page: textContentSource.page, container: container.className, viewport });
  }

  render(): Promise<void> {
    return Promise.resolve();
  }

  cancel(): void {}
}

mock.module("pdfjs-dist/legacy/build/pdf.mjs", () => ({
  GlobalWorkerOptions: fakePdfjs.worker,
  PixelsPerInch: { PDF_TO_CSS_UNITS: CSS_UNITS },
  TextLayer: FakeTextLayer,
  getDocument: (options: { url: string }) => {
    fakePdfjs.loads.push(options);
    const found = fakePdfjs.documents.get(options.url);
    const promise =
      found === undefined || typeof found === "string"
        ? Promise.reject(new Error(found ?? `Missing PDF "${options.url}".`))
        : Promise.resolve({
            numPages: found.pages,
            getPage: (number: number) => Promise.resolve(fakePage(number, found)),
          });
    // The viewer always handles it; this keeps an unawaited rejection from failing the run.
    promise.catch(() => undefined);
    return {
      promise,
      destroy: () => {
        fakePdfjs.destroyed.push(options.url);
        return Promise.resolve();
      },
    };
  },
}));

/**
 * An `IntersectionObserver` for which every observed element is near the view until `far` says otherwise. Install it
 * with `globalThis.IntersectionObserver = nearObserver.Observer` and restore happy-dom's after the test.
 */
export const nearObserver = {
  observers: new Set<{ callback: IntersectionObserverCallback; targets: Set<Element> }>(),
  Observer: class {
    readonly #entry: { callback: IntersectionObserverCallback; targets: Set<Element> };

    constructor(callback: IntersectionObserverCallback) {
      this.#entry = { callback, targets: new Set() };
      nearObserver.observers.add(this.#entry);
    }

    observe(target: Element): void {
      this.#entry.targets.add(target);
      this.#report(target, true);
    }

    unobserve(target: Element): void {
      this.#entry.targets.delete(target);
    }

    disconnect(): void {
      nearObserver.observers.delete(this.#entry);
    }

    takeRecords(): IntersectionObserverEntry[] {
      return [];
    }

    #report(target: Element, isIntersecting: boolean): void {
      const entry = { target, isIntersecting } as IntersectionObserverEntry;
      this.#entry.callback([entry], this as unknown as IntersectionObserver);
    }
  },
  /** Tells every observer of `target` that it left the rendering distance. */
  far(target: Element): void {
    for (const { callback, targets } of nearObserver.observers) {
      if (targets.has(target)) {
        callback([{ target, isIntersecting: false } as IntersectionObserverEntry], {} as IntersectionObserver);
      }
    }
  },
};
