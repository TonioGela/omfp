/** L4: the kit's PDF viewer over a fake pdf.js (UI-543..UI-546), its zoom steps and its page indicator. */

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PdfView, type PdfViewLabels } from "../src/index.ts";
import { outputRatio, pageAt } from "../src/pdf.tsx";
import { fitScale, zoomIn, zoomLabel, zoomOut } from "../src/zoom.ts";
import { findRule, KIT_SRC, parseCss } from "./support/css.ts";
import { fakePdfjs, nearObserver } from "./support/pdfjs.ts";

const LABELS: PdfViewLabels = {
  loading: "Loading the PDF…",
  failed: (error) => `Cannot show this PDF: ${error}`,
  zoomIn: "Zoom in",
  zoomOut: "Zoom out",
  fitWidth: "Fit width",
  page: (page, pages) => `${page} / ${pages}`,
  pageLabel: (page) => `Page ${page}`,
  toolbar: "Zoom and pages",
  pages: "Pages",
};

/** A US Letter page, 612 × 792 points: 816 × 1056 CSS pixels at 100%. */
const LETTER = { pages: 3, width: 612, height: 792 };

const happyDomObserver = globalThis.IntersectionObserver;

beforeEach(() => {
  fakePdfjs.reset();
  globalThis.IntersectionObserver = nearObserver.Observer as unknown as typeof IntersectionObserver;
});

afterEach(() => {
  cleanup();
  globalThis.IntersectionObserver = happyDomObserver;
});

function pageSizes(): string[] {
  return screen.getAllByTestId("pdf-page").map((page) => `${page.style.inlineSize} × ${page.style.blockSize}`);
}

describe("PdfView", () => {
  // spec: UI-543
  test("loads the document by URL with pdf.js's files from /pdfjs/, then lays out every page and draws those near the view", async () => {
    fakePdfjs.documents.set("/files/k/w1/doc.pdf?v=1", LETTER);
    render(<PdfView url="/files/k/w1/doc.pdf?v=1" labels={LABELS} aria-label="PDF doc.pdf" />);
    expect(screen.getByTestId("pdf-view").textContent).toBe("Loading the PDF…");
    await waitFor(() => expect(screen.getAllByTestId("pdf-page").length).toBe(3));

    expect(fakePdfjs.worker.workerSrc).toBe("/pdfjs/pdf.worker.min.mjs");
    expect(fakePdfjs.loads).toEqual([
      {
        url: "/files/k/w1/doc.pdf?v=1",
        cMapUrl: "/pdfjs/cmaps/",
        cMapPacked: true,
        standardFontDataUrl: "/pdfjs/standard_fonts/",
        wasmUrl: "/pdfjs/wasm/",
        iccUrl: "/pdfjs/iccs/",
      },
    ]);
    // happy-dom lays nothing out: the viewer is 0 px wide, so fitting the width gives 100%.
    expect(pageSizes()).toEqual(["816px × 1056px", "816px × 1056px", "816px × 1056px"]);
    expect(screen.getAllByTestId("pdf-page").map((page) => page.getAttribute("aria-label"))).toEqual([
      "Page 1",
      "Page 2",
      "Page 3",
    ]);
    await waitFor(() =>
      expect(screen.getAllByTestId("pdf-page").map((page) => page.dataset.rendered)).toEqual(["true", "true", "true"]),
    );
    expect(fakePdfjs.renders.map(({ page, canvasWidth }) => [page, canvasWidth])).toEqual([
      [1, 816],
      [2, 816],
      [3, 816],
    ]);

    // A page leaving the rendering distance gives its canvas's memory back.
    const second = screen.getAllByTestId("pdf-page")[1];
    if (second === undefined) throw new Error("no second page");
    act(() => nearObserver.far(second));
    expect(second.dataset.rendered).toBe("false");
    expect(second.querySelector("canvas")?.width).toBe(0);

    cleanup();
    expect(fakePdfjs.destroyed).toEqual(["/files/k/w1/doc.pdf?v=1"]);
  });

  // spec: UI-543
  test("a document pdf.js cannot read says why; a new URL loads again", async () => {
    fakePdfjs.documents.set("/bad.pdf", "Invalid PDF structure.");
    const { rerender } = render(<PdfView url="/bad.pdf" labels={LABELS} />);
    expect((await screen.findByTestId("pdf-error")).textContent).toBe("Cannot show this PDF: Invalid PDF structure.");
    fakePdfjs.documents.set("/good.pdf", { ...LETTER, pages: 1 });
    rerender(<PdfView url="/good.pdf" labels={LABELS} />);
    await waitFor(() => expect(screen.getAllByTestId("pdf-page").length).toBe(1));
    expect(screen.queryAllByTestId("pdf-error").length).toBe(0);
  });

  // spec: UI-544
  test("zoom out and in by steps, fit width again; pages and their drawing follow the scale", async () => {
    fakePdfjs.documents.set("/doc.pdf", { ...LETTER, pages: 1 });
    const user = userEvent.setup();
    render(<PdfView url="/doc.pdf" labels={LABELS} />);
    const zoom = await screen.findByTestId("pdf-zoom");
    const fit = screen.getByTestId("pdf-fit");
    expect([zoom.textContent, fit.getAttribute("aria-pressed")]).toEqual(["100%", "true"]);

    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    expect([zoom.textContent, fit.getAttribute("aria-pressed")]).toEqual(["125%", "false"]);
    expect(pageSizes()).toEqual(["1020px × 1320px"]);
    await waitFor(() => expect(fakePdfjs.renders.at(-1)?.viewport.width).toBeCloseTo(1020));

    await user.click(screen.getByRole("button", { name: "Zoom out" }));
    await user.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(zoom.textContent).toBe("75%");
    expect(pageSizes()).toEqual(["612px × 792px"]);

    await user.click(fit);
    expect([zoom.textContent, fit.getAttribute("aria-pressed")]).toEqual(["100%", "true"]);
  });

  // spec: UI-544
  test("zoom steps from 25% to 400%, from any scale; fitting a width stays within them", () => {
    expect([zoomIn(1), zoomIn(1.37), zoomIn(4), zoomIn(0.999)]).toEqual([1.25, 1.5, 4, 1.25]);
    expect([zoomOut(1), zoomOut(1.37), zoomOut(0.25), zoomOut(1.001)]).toEqual([0.75, 1.25, 0.25, 0.75]);
    expect([fitScale(408, 816), fitScale(10_000, 816), fitScale(50, 816), fitScale(0, 816)]).toEqual([0.5, 4, 0.25, 1]);
    expect([zoomLabel(1), zoomLabel(1.337), zoomLabel(0.25)]).toEqual(["100%", "134%", "25%"]);
  });

  // spec: UI-543
  test("a page's canvas has the screen's pixels, fewer when that would make it bigger than 4096²", () => {
    expect(outputRatio(816, 1056, 2)).toBe(2);
    expect(outputRatio(816, 1056, 0)).toBe(1);
    const coarse = outputRatio(3264, 4224, 2);
    expect(coarse).toBeLessThan(2);
    expect(Math.round(3264 * coarse * 4224 * coarse)).toBe(4096 * 4096);
  });

  // spec: UI-545
  test("the page indicator names the page at the top of the view, of how many", async () => {
    fakePdfjs.documents.set("/doc.pdf", LETTER);
    render(<PdfView url="/doc.pdf" labels={LABELS} />);
    const indicator = await screen.findByTestId("pdf-page-number");
    expect([indicator.textContent, indicator.getAttribute("aria-live")]).toEqual(["1 / 3", "polite"]);
    // Pages starting at 0, 1068 and 2136 px: the page that holds the line a third down the view.
    expect([pageAt([0, 1068, 2136], 0), pageAt([0, 1068, 2136], 1068), pageAt([0, 1068, 2136], 5000)]).toEqual([
      1, 2, 3,
    ]);
    expect([pageAt([12, 1080], 0), pageAt([], 100)]).toEqual([1, 1]);
  });

  // spec: UI-546
  test("each drawn page gets pdf.js's text layer at its scale, its words transparent and selectable", async () => {
    fakePdfjs.documents.set("/doc.pdf", { ...LETTER, pages: 2 });
    render(<PdfView url="/doc.pdf" labels={LABELS} />);
    await waitFor(() => expect(fakePdfjs.textLayers.length).toBe(2));
    expect(fakePdfjs.textLayers.map(({ page, container, viewport }) => [page, container, viewport.width])).toEqual([
      [1, "textLayer", 816],
      [2, "textLayer", 816],
    ]);
    const page = screen.getAllByTestId("pdf-page")[0];
    expect(page?.style.getPropertyValue("--total-scale-factor")).toBe(String(96 / 72));

    const rules = parseCss(readFileSync(join(KIT_SRC, "kit.css"), "utf8"));
    const words = findRule(rules, ["@layer omfp-kit"], ".omfp-pdf .textLayer :is(span, br)");
    expect([words.declarations.get("color"), words.declarations.get("user-select")]).toEqual(["transparent", "text"]);
    const layer = findRule(rules, ["@layer omfp-kit"], ".omfp-pdf .textLayer");
    expect([layer.declarations.get("position"), layer.declarations.get("inset")]).toEqual(["absolute", "0"]);
  });
});
