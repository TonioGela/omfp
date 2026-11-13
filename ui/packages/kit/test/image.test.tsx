/** L4: the kit's image viewer (UI-549..UI-555): fit and zoom, gestures, panning, and comparing two versions. */

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ImageView, type ImageViewLabels } from "../src/index.ts";
import { anchoredScroll, containScale, IMAGE_ZOOM_STEPS, wheelZoom, zoomIn, zoomOut } from "../src/zoom.ts";

const LABELS: ImageViewLabels = {
  zoomIn: "Zoom in",
  zoomOut: "Zoom out",
  fit: "Fit",
  actualSize: "100%",
  toolbar: "Zoom",
  failed: "This window cannot show this image.",
  before: "HEAD",
  after: "Working copy",
  slider: "HEAD | working copy",
};

/** happy-dom lays nothing out: the viewports measure 400 × 300 for these tests. */
const VIEW = { width: 400, height: 300 };
const measured = ["clientWidth", "clientHeight"] as const;
const original = measured.map((name) => Object.getOwnPropertyDescriptor(window.HTMLElement.prototype, name));

beforeEach(() => {
  for (const name of measured) {
    Object.defineProperty(window.HTMLElement.prototype, name, {
      configurable: true,
      get(this: HTMLElement) {
        if (!this.classList.contains("omfp-image__viewport")) {
          return 0;
        }
        return name === "clientWidth" ? VIEW.width : VIEW.height;
      },
    });
  }
});

afterEach(() => {
  cleanup();
  measured.forEach((name, index) => {
    const descriptor = original[index];
    if (descriptor !== undefined) {
      Object.defineProperty(window.HTMLElement.prototype, name, descriptor);
    }
  });
});

/** Tells the viewer the image `testId` loaded with its natural size. */
function load(testId: string, width: number, height: number): void {
  const image = screen.getByTestId(testId);
  Object.defineProperty(image, "naturalWidth", { configurable: true, value: width });
  Object.defineProperty(image, "naturalHeight", { configurable: true, value: height });
  fireEvent.load(image);
}

function box(testId: string): string {
  const { left, top, inlineSize, blockSize } = screen.getByTestId(testId).style;
  return `${left} ${top} ${inlineSize} × ${blockSize}`;
}

function pointer(type: string, target: Element, pointerId: number, clientX: number, clientY: number): void {
  act(() => {
    target.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId, clientX, clientY, button: 0 }));
  });
}

describe("ImageView", () => {
  // spec: UI-549
  test("fits the whole image by default, never above 100%; zooms by steps, to 100% and back to fit", async () => {
    const user = userEvent.setup();
    render(<ImageView src="/files/k/w1/photo.png?v=1" alt="photo.png" labels={LABELS} />);
    load("image-after", 800, 600);
    const zoom = screen.getByTestId("image-zoom");
    const fit = screen.getByTestId("image-fit");
    expect([zoom.textContent, fit.getAttribute("aria-pressed")]).toEqual(["50%", "true"]);
    expect(box("image-after")).toBe("0px 0px 400px × 300px");

    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    expect([zoom.textContent, fit.getAttribute("aria-pressed")]).toEqual(["75%", "false"]);
    await user.click(screen.getByTestId("image-actual"));
    expect(zoom.textContent).toBe("100%");
    expect(box("image-after")).toBe("0px 0px 800px × 600px");
    const stage = screen.getByTestId("image-viewport").querySelector<HTMLElement>(".omfp-image__box");
    if (stage === null) throw new Error("no box");
    expect([stage.style.inlineSize, stage.style.blockSize]).toEqual(["800px", "600px"]);
    await user.click(screen.getByRole("button", { name: "Zoom out" }));
    expect(zoom.textContent).toBe("75%");
    await user.click(fit);
    expect([zoom.textContent, fit.getAttribute("aria-pressed")]).toEqual(["50%", "true"]);
    cleanup();

    // A small image keeps its pixels, centred.
    render(<ImageView src="/icon.png" alt="icon.png" labels={LABELS} />);
    load("image-after", 40, 20);
    expect([screen.getByTestId("image-zoom").textContent, box("image-after")]).toEqual(["100%", "0px 0px 40px × 20px"]);
  });

  // spec: UI-549
  test("the zoom steps go from 5% to 1600%; fitting stays within them and below 100%", () => {
    expect([IMAGE_ZOOM_STEPS[0], IMAGE_ZOOM_STEPS.at(-1)]).toEqual([0.05, 16]);
    expect([zoomIn(1, IMAGE_ZOOM_STEPS), zoomIn(16, IMAGE_ZOOM_STEPS), zoomOut(0.07, IMAGE_ZOOM_STEPS)]).toEqual([
      1.5, 16, 0.05,
    ]);
    expect(containScale(VIEW, { width: 800, height: 300 })).toBe(0.5);
    expect(containScale(VIEW, { width: 100_000, height: 10 })).toBe(0.05);
    expect(containScale(VIEW, { width: 0, height: 0 })).toBe(1);
  });

  // spec: UI-550
  test("Ctrl with the wheel zooms smoothly without zooming the page; two pointers pinch", async () => {
    const user = userEvent.setup();
    render(<ImageView src="/photo.png" alt="photo.png" labels={LABELS} />);
    load("image-after", 800, 600);
    const viewport = screen.getByTestId("image-viewport");
    const plain = new WheelEvent("wheel", { deltaY: -500, cancelable: true, bubbles: true });
    act(() => {
      viewport.dispatchEvent(plain);
    });
    expect([plain.defaultPrevented, screen.getByTestId("image-zoom").textContent]).toEqual([false, "50%"]);
    const zooming = new WheelEvent("wheel", { deltaY: -500, ctrlKey: true, cancelable: true, bubbles: true });
    // happy-dom's WheelEvent takes no modifier keys nor position from its init.
    for (const [name, value] of [
      ["ctrlKey", true],
      ["clientX", 200],
      ["clientY", 150],
    ] as const) {
      Object.defineProperty(zooming, name, { value });
    }
    act(() => {
      viewport.dispatchEvent(zooming);
    });
    expect([zooming.defaultPrevented, screen.getByTestId("image-zoom").textContent]).toEqual([true, "136%"]);
    expect(wheelZoom(1, 500)).toBeCloseTo(Math.exp(-1));
    expect(wheelZoom(16, -500)).toBe(16);

    // Two fingers 100 px apart, then 200 px: twice the scale.
    await user.click(screen.getByTestId("image-fit"));
    pointer("pointerdown", viewport, 1, 100, 100);
    pointer("pointerdown", viewport, 2, 200, 100);
    pointer("pointermove", viewport, 2, 300, 100);
    expect(screen.getByTestId("image-zoom").textContent).toBe("100%");
    pointer("pointerup", viewport, 1, 100, 100);
    pointer("pointerup", viewport, 2, 300, 100);
    expect(viewport.dataset.dragging).toBeUndefined();
  });

  // spec: UI-551
  test("dragging pans; a zoom keeps the point under the pointer in place", () => {
    render(<ImageView src="/photo.png" alt="photo.png" labels={LABELS} />);
    load("image-after", 800, 600);
    const viewport = screen.getByTestId("image-viewport");
    pointer("pointerdown", viewport, 1, 100, 100);
    expect(viewport.dataset.dragging).toBe("true");
    const scrolled: [number, number][] = [];
    let left = 50;
    let top = 50;
    Object.defineProperty(viewport, "scrollLeft", {
      configurable: true,
      get: () => left,
      set: (value: number) => {
        left = value;
        scrolled.push([left, top]);
      },
    });
    Object.defineProperty(viewport, "scrollTop", {
      configurable: true,
      get: () => top,
      set: (value: number) => {
        top = value;
        scrolled.push([left, top]);
      },
    });
    pointer("pointermove", viewport, 1, 60, 70);
    expect([left, top]).toEqual([90, 80]);
    pointer("pointerup", viewport, 1, 60, 70);
    expect(viewport.dataset.dragging).toBeUndefined();

    // 800 px at 100% in a 400 px view, scrolled to 100: the content point under x = 200 (at 300) stays under it at 200%.
    expect(anchoredScroll(100, 200, 400, 800, 1, 2)).toBe(400);
    // Content smaller than the view is centred: nothing to scroll.
    expect(anchoredScroll(0, 200, 400, 800, 0.25, 0.5)).toBe(0);
    expect(anchoredScroll(0, 0, 400, 800, 0.5, 4)).toBe(0);
    expect(anchoredScroll(400, 400, 400, 800, 1, 0.5)).toBe(0);
  });

  // spec: UI-552
  test("an image the browser cannot show says so; another source tries again", () => {
    const { rerender } = render(<ImageView src="/broken.png" alt="broken.png" labels={LABELS} />);
    fireEvent.error(screen.getByTestId("image-after"));
    expect(screen.getByTestId("image-error").textContent).toBe("This window cannot show this image.");
    rerender(<ImageView src="/broken.png?v=2" alt="broken.png" labels={LABELS} />);
    expect(screen.queryAllByTestId("image-error").length).toBe(0);
    expect(screen.getByTestId("image-after").getAttribute("src")).toBe("/broken.png?v=2");
  });

  // spec: UI-554
  test("side by side: HEAD left, the working copy right, at the scale that fits both, scrolled together", () => {
    render(
      <ImageView
        src="/now.png"
        alt="now.png"
        before={{ src: "/head.png", alt: "now.png at HEAD" }}
        compare="split"
        labels={LABELS}
      />,
    );
    load("image-before", 400, 300);
    load("image-after", 800, 300);
    const viewports = screen.getAllByTestId("image-viewport");
    expect(viewports.map((viewport) => [viewport.dataset.side, viewport.getAttribute("aria-label")])).toEqual([
      ["before", "HEAD"],
      ["after", "Working copy"],
    ]);
    expect(screen.getByTestId("image-view").dataset.compare).toBe("split");
    // The box of both is 800 × 300: 50% fits it; each image centred in it.
    expect(screen.getByTestId("image-zoom").textContent).toBe("50%");
    expect([box("image-before"), box("image-after")]).toEqual(["100px 0px 200px × 150px", "0px 0px 400px × 150px"]);
    const [head, now] = viewports;
    if (head === undefined || now === undefined) throw new Error("two viewports");
    const scrolls: string[] = [];
    head.scrollTo = (options?: ScrollToOptions | number) => {
      scrolls.push(JSON.stringify(options));
    };
    Object.defineProperty(now, "scrollLeft", { configurable: true, value: 30 });
    fireEvent.scroll(now);
    expect(scrolls).toEqual([JSON.stringify({ left: 30, top: 0 })]);
  });

  // spec: UI-555
  test("slider: the working copy over HEAD, cut at the slider, which moves", () => {
    render(
      <ImageView
        src="/now.png"
        alt="now.png"
        before={{ src: "/head.png", alt: "now.png at HEAD" }}
        compare="slider"
        labels={LABELS}
      />,
    );
    load("image-before", 800, 600);
    load("image-after", 800, 600);
    expect(screen.getAllByTestId("image-viewport").map((viewport) => viewport.dataset.side)).toEqual(["both"]);
    const slider = screen.getByRole("slider", { name: "HEAD | working copy" });
    expect(slider.getAttribute("value")).toBe("50");
    expect(screen.getByTestId("image-divider").style.left).toBe("200px");
    expect(screen.getByTestId("image-after").style.clipPath).toBe("inset(0 0 0 200px)");
    expect(screen.getByTestId("image-before").style.clipPath).toBe("");
    fireEvent.change(slider, { target: { value: "25" } });
    expect(screen.getByTestId("image-divider").style.left).toBe("100px");
    expect(screen.getByTestId("image-after").style.clipPath).toBe("inset(0 0 0 100px)");
  });
});
