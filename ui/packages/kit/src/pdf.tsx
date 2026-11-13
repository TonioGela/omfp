/**
 * PDF documents in the kit (UI-543..UI-546): pdf.js (`pdfjs-dist`) renders the pages of a document loaded by URL into
 * canvases with a text layer, in a chunk of the bundle loaded on first use; its worker and data files are served by
 * the bundle at `/pdfjs/` (build.ts). WebKitGTK has no PDF viewer of its own, so an `<iframe>` would show nothing.
 */
import { type HTMLAttributes, lazy, type ReactNode, Suspense } from "react";
import { cx } from "./class.ts";

/** Loading pdf.js is a code-splitting point of the bundle: it cannot be a static import. */
const PdfDocumentView = lazy(() => import("./pdf/viewer.tsx").then((module) => ({ default: module.PdfDocumentView })));

/** What the viewer says, in the window's language. */
export interface PdfViewLabels {
  /** While the document loads. */
  readonly loading: string;
  /** A document that cannot be shown (not a PDF, damaged, encrypted, unreachable). */
  readonly failed: (error: string) => string;
  readonly zoomIn: string;
  readonly zoomOut: string;
  /** The toggle that keeps the pages as wide as the viewer. */
  readonly fitWidth: string;
  /** The page shown at the top of the viewer, of how many: `3 / 12`. */
  readonly page: (page: number, pages: number) => string;
  /** The name of page `page` (its region). */
  readonly pageLabel: (page: number) => string;
  /** The viewer's toolbar. */
  readonly toolbar: string;
  /** The scrolling list of pages. */
  readonly pages: string;
}

/**
 * The page shown at `position` (a scroll offset) of pages whose boxes start at `tops`, in order: the last one starting
 * at or above it, 1-based; the first page above them all.
 */
export function pageAt(tops: readonly number[], position: number): number {
  const below = tops.findIndex((top) => top > position);
  return below === -1 ? Math.max(1, tops.length) : Math.max(1, below);
}

/** The most device pixels of one page's canvas (4096²): beyond, the canvas is drawn coarser and scaled up. */
const MAX_CANVAS_PIXELS = 4096 * 4096;

/** Device pixels per CSS pixel of a canvas `width` × `height` CSS pixels: the screen's, unless it makes it too big. */
export function outputRatio(width: number, height: number, devicePixelRatio: number): number {
  const ratio = devicePixelRatio > 0 ? devicePixelRatio : 1;
  const area = width * height;
  return area * ratio * ratio <= MAX_CANVAS_PIXELS ? ratio : Math.sqrt(MAX_CANVAS_PIXELS / area);
}

type RootAttributes = Omit<HTMLAttributes<HTMLDivElement>, "children">;

export interface PdfViewProps extends RootAttributes {
  /** Where the document is, same-origin: e.g. `useFileUrl(path)`. pdf.js reads it with byte ranges. */
  readonly url: string;
  readonly labels: PdfViewLabels;
}

/**
 * A PDF document: its pages one under the other, scrolling inside the viewer, rendered when near the view; zoom out,
 * in and fit width (the default) in a toolbar with the page at the top; text selectable over each page. It fills the
 * height its container gives it.
 */
export function PdfView({ url, labels, className, ...props }: PdfViewProps): ReactNode {
  return (
    <div data-testid="pdf-view" {...props} className={cx("omfp-pdf", className)} data-url={url}>
      <Suspense fallback={<p className="omfp-pdf__notice">{labels.loading}</p>}>
        <PdfDocumentView url={url} labels={labels} />
      </Suspense>
    </div>
  );
}
