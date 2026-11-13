/**
 * The pdf.js chunk of `PdfView` (UI-543..UI-546): loads the document, lays its pages out at the zoom's scale, and
 * renders each page — canvas and text layer — only while it is near the view. pdf.js's legacy build: its modern one
 * calls `Map.prototype.getOrInsertComputed`, which the WebKitGTK of `omfp desktop` and older browsers lack.
 */
import {
  GlobalWorkerOptions,
  getDocument,
  type PDFDocumentProxy,
  type PDFPageProxy,
  PixelsPerInch,
  TextLayer,
} from "pdfjs-dist/legacy/build/pdf.mjs";
import {
  type CSSProperties,
  type ReactNode,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { Button, IconButton } from "../button.tsx";
import { Icon } from "../icons.tsx";
import { outputRatio, type PdfViewLabels, pageAt } from "../pdf.tsx";
import { fitScale, zoomIn, zoomLabel, zoomOut } from "../zoom.ts";

/** Where the bundle serves pdf.js's worker and data files (build.ts). */
const ASSETS = "/pdfjs/";
GlobalWorkerOptions.workerSrc = `${ASSETS}pdf.worker.min.mjs`;

/** CSS pixels per PDF point at 100%. */
const CSS_UNITS = PixelsPerInch.PDF_TO_CSS_UNITS;
/** Pages rendered ahead of the view, as a fraction of its height on each side. */
const AHEAD = "100% 0px";

/** A page's size at 100%, in CSS pixels. */
interface Size {
  readonly width: number;
  readonly height: number;
}

type Loaded =
  | { readonly url: string; readonly document: PDFDocumentProxy; readonly first: Size }
  | { readonly url: string; readonly error: string };

/** The document at `url` with the size of its first page, or why it cannot be shown; `undefined` while it loads. */
function useDocument(url: string): Loaded | undefined {
  const [loaded, setLoaded] = useState<Loaded | undefined>(undefined);
  useEffect(() => {
    const task = getDocument({
      url,
      cMapUrl: `${ASSETS}cmaps/`,
      cMapPacked: true,
      standardFontDataUrl: `${ASSETS}standard_fonts/`,
      wasmUrl: `${ASSETS}wasm/`,
      iccUrl: `${ASSETS}iccs/`,
    });
    let live = true;
    task.promise
      .then(async (document) => {
        const { width, height } = (await document.getPage(1)).getViewport({ scale: CSS_UNITS });
        if (live) {
          setLoaded({ url, document, first: { width, height } });
        }
      })
      .catch((error: unknown) => {
        if (live) {
          setLoaded({ url, error: error instanceof Error ? error.message : String(error) });
        }
      });
    return () => {
      live = false;
      void task.destroy();
    };
  }, [url]);
  return loaded?.url === url ? loaded : undefined;
}

export function PdfDocumentView({ url, labels }: { url: string; labels: PdfViewLabels }): ReactNode {
  const loaded = useDocument(url);
  if (loaded === undefined) {
    return <p className="omfp-pdf__notice">{labels.loading}</p>;
  }
  if ("error" in loaded) {
    return (
      <p className="omfp-pdf__notice" data-tone="error" data-testid="pdf-error">
        {labels.failed(loaded.error)}
      </p>
    );
  }
  return <Pages document={loaded.document} first={loaded.first} labels={labels} />;
}

/** The width of the content box of `element` (inside its padding), live. */
function useContentWidth(element: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const target = element.current;
    if (target === null) {
      return;
    }
    const measure = () => {
      const style = getComputedStyle(target);
      const padding = (Number.parseFloat(style.paddingLeft) || 0) + (Number.parseFloat(style.paddingRight) || 0);
      setWidth(target.clientWidth - padding);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(target);
    return () => observer.disconnect();
  }, [element]);
  return width;
}

function Pages({
  document,
  first,
  labels,
}: {
  document: PDFDocumentProxy;
  first: Size;
  labels: PdfViewLabels;
}): ReactNode {
  const scroller = useRef<HTMLElement>(null);
  const width = useContentWidth(scroller);
  const [fit, setFit] = useState(true);
  const [chosen, setChosen] = useState(1);
  const scale = fit ? fitScale(width, first.width) : chosen;
  const [current, setCurrent] = useState(1);
  const pages = document.numPages;

  // A new scale keeps the same part of the document at the top.
  const shown = useRef(scale);
  useLayoutEffect(() => {
    const element = scroller.current;
    if (element !== null && shown.current !== scale) {
      element.scrollTop *= scale / shown.current;
    }
    shown.current = scale;
  }, [scale]);

  const onScroll = () => {
    const element = scroller.current;
    if (element === null) {
      return;
    }
    const tops = [...element.querySelectorAll<HTMLElement>("[data-page]")].map((page) => page.offsetTop);
    setCurrent(pageAt(tops, element.scrollTop + element.clientHeight / 3));
  };
  const zoomTo = (next: number) => {
    setFit(false);
    setChosen(next);
  };

  return (
    <>
      <div className="omfp-pdf__bar" role="toolbar" aria-label={labels.toolbar}>
        <IconButton
          label={labels.zoomOut}
          icon={<Icon name="minus" />}
          onPress={() => zoomTo(zoomOut(scale))}
          data-testid="pdf-zoom-out"
        />
        <span className="omfp-pdf__zoom" data-testid="pdf-zoom">
          {zoomLabel(scale)}
        </span>
        <IconButton
          label={labels.zoomIn}
          icon={<Icon name="plus" />}
          onPress={() => zoomTo(zoomIn(scale))}
          data-testid="pdf-zoom-in"
        />
        <Button size="sm" variant="quiet" aria-pressed={fit} onPress={() => setFit(true)} data-testid="pdf-fit">
          {labels.fitWidth}
        </Button>
        <span className="omfp-pdf__page-number" aria-live="polite" data-testid="pdf-page-number">
          {labels.page(current, pages)}
        </span>
      </div>
      <section
        ref={scroller}
        className="omfp-pdf__pages omfp-focus-inset"
        aria-label={labels.pages}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrolling region the keyboard must reach to scroll (UI-092).
        tabIndex={0}
        onScroll={onScroll}
        data-testid="pdf-pages"
      >
        {Array.from({ length: pages }, (_, index) => (
          <Page
            // biome-ignore lint/suspicious/noArrayIndexKey: pages are their numbers, never reordered.
            key={index}
            document={document}
            number={index + 1}
            scale={scale}
            fallback={first}
            root={scroller}
            label={labels.pageLabel(index + 1)}
          />
        ))}
      </section>
    </>
  );
}

/** Whether `element` is within the rendering distance of the view `root`, live. */
function useNear(element: RefObject<HTMLElement | null>, root: RefObject<HTMLElement | null>): boolean {
  const [near, setNear] = useState(false);
  useEffect(() => {
    const target = element.current;
    if (target === null) {
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        const entry = entries.at(-1);
        if (entry !== undefined) {
          setNear(entry.isIntersecting);
        }
      },
      { root: root.current, rootMargin: AHEAD },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, [element, root]);
  return near;
}

interface PageProps {
  readonly document: PDFDocumentProxy;
  readonly number: number;
  readonly scale: number;
  /** The size until the page itself loads: the first page's. */
  readonly fallback: Size;
  readonly root: RefObject<HTMLElement | null>;
  readonly label: string;
}

/** One page: its box at the scale always, its canvas and text only while near the view (released when far). */
function Page({ document, number, scale, fallback, root, label }: PageProps): ReactNode {
  const box = useRef<HTMLElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const text = useRef<HTMLDivElement>(null);
  const near = useNear(box, root);
  const [page, setPage] = useState<PDFPageProxy | undefined>(undefined);
  const [rendered, setRendered] = useState(false);

  useEffect(() => {
    if (!near || page !== undefined) {
      return;
    }
    let live = true;
    void document.getPage(number).then((loaded) => {
      if (live) {
        setPage(loaded);
      }
    });
    return () => {
      live = false;
    };
  }, [document, near, number, page]);

  useEffect(() => {
    const [surface, layer] = [canvas.current, text.current];
    if (!near || page === undefined || surface === null || layer === null) {
      return;
    }
    const viewport = page.getViewport({ scale: scale * CSS_UNITS });
    const ratio = outputRatio(viewport.width, viewport.height, window.devicePixelRatio);
    surface.width = Math.floor(viewport.width * ratio);
    surface.height = Math.floor(viewport.height * ratio);
    const task = page.render({
      canvas: surface,
      viewport,
      ...(ratio === 1 ? {} : { transform: [ratio, 0, 0, ratio, 0, 0] }),
    });
    layer.replaceChildren();
    const words = new TextLayer({ textContentSource: page.streamTextContent(), container: layer, viewport });
    let live = true;
    Promise.all([task.promise, words.render()]).then(
      () => live && setRendered(true),
      // Cancelled by a new scale or by leaving the view.
      () => undefined,
    );
    return () => {
      live = false;
      task.cancel();
      words.cancel();
      setRendered(false);
      // Releases the canvas's memory while the page is out of view.
      surface.width = 0;
      surface.height = 0;
      layer.replaceChildren();
    };
  }, [near, page, scale]);

  const size = page === undefined ? fallback : page.getViewport({ scale: CSS_UNITS });
  const factor = scale * CSS_UNITS;
  const style = {
    inlineSize: `${size.width * scale}px`,
    blockSize: `${size.height * scale}px`,
    // What pdf.js's text layer sizes its words with (pdf_viewer.css's variables).
    "--scale-factor": factor,
    "--total-scale-factor": factor,
    "--scale-round-x": "1px",
    "--scale-round-y": "1px",
  } as CSSProperties;
  return (
    <section
      ref={box}
      className="omfp-pdf__page"
      aria-label={label}
      style={style}
      data-page={number}
      data-rendered={rendered}
      data-testid="pdf-page"
    >
      <canvas ref={canvas} className="omfp-pdf__canvas" />
      <div ref={text} className="textLayer" />
    </section>
  );
}
