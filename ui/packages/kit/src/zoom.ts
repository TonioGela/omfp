/**
 * Zoom of the kit's document and image viewers (`PdfView`, UI-544; `ImageView`, UI-549..UI-551): scales as fractions
 * of the actual size (1 = 100%), the steps the zoom buttons go through, the scale that fits a width or a box, and the
 * scroll that keeps a point under the pointer while the scale changes.
 */

/** The scales the PDF viewer's zoom buttons step through, smallest first. */
export const ZOOM_STEPS: readonly number[] = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

/** The scales the image viewer's zoom buttons step through, smallest first: down to thumbnails, up to pixels. */
export const IMAGE_ZOOM_STEPS: readonly number[] = [0.05, 0.1, 0.25, 0.5, 0.75, 1, 1.5, 2, 3, 4, 6, 8, 12, 16];

/** Two scales this close are the same step (a fitted scale near a step goes to the next one). */
const SAME = 0.005;

function bounds(steps: readonly number[]): [number, number] {
  return [steps[0] ?? 1, steps.at(-1) ?? 1];
}

/** `scale` within the smallest and the largest of `steps`. */
export function clampZoom(scale: number, steps: readonly number[] = ZOOM_STEPS): number {
  const [min, max] = bounds(steps);
  return Math.min(max, Math.max(min, scale));
}

/** The next step above `scale`, or the largest. */
export function zoomIn(scale: number, steps: readonly number[] = ZOOM_STEPS): number {
  return steps.find((step) => step > scale + SAME) ?? bounds(steps)[1];
}

/** The next step below `scale`, or the smallest. */
export function zoomOut(scale: number, steps: readonly number[] = ZOOM_STEPS): number {
  return steps.findLast((step) => step < scale - SAME) ?? bounds(steps)[0];
}

/** The scale at which something `width` wide at 100% fills `available` pixels, within the zoom's bounds. */
export function fitScale(available: number, width: number): number {
  if (!(available > 0 && width > 0)) {
    return 1;
  }
  return clampZoom(available / width);
}

/** A width and a height, in CSS pixels. */
export interface Size {
  readonly width: number;
  readonly height: number;
}

/**
 * The scale at which `content` (its size at 100%) fits whole in `view`, never above 100% (a small image keeps its
 * pixels), within the bounds of `steps`; 1 while either is unknown (empty).
 */
export function containScale(view: Size, content: Size, steps: readonly number[] = IMAGE_ZOOM_STEPS): number {
  if (!(view.width > 0 && view.height > 0 && content.width > 0 && content.height > 0)) {
    return 1;
  }
  return clampZoom(Math.min(1, view.width / content.width, view.height / content.height), steps);
}

/** The scale after a wheel turn of `deltaY` pixels with Ctrl held (or a trackpad pinch): smooth, up when negative. */
export function wheelZoom(scale: number, deltaY: number, steps: readonly number[] = IMAGE_ZOOM_STEPS): number {
  return clampZoom(scale * Math.exp(-deltaY / 500), steps);
}

/** Where content `size` (at the scale) starts in a view `view` wide or tall: centred when smaller, else at 0. */
export function centredOffset(view: number, size: number): number {
  return Math.max(0, (view - size) / 2);
}

/**
 * The scroll offset along one axis that keeps the content point under `point` (a position in a view `view` long)
 * where it is when content `natural` long at 100% goes from scale `from` to `to`, scrolled at `scroll` before.
 */
export function anchoredScroll(
  scroll: number,
  point: number,
  view: number,
  natural: number,
  from: number,
  to: number,
): number {
  const at = (scroll + point - centredOffset(view, natural * from)) / from;
  const next = at * to + centredOffset(view, natural * to) - point;
  return Math.min(Math.max(0, next), Math.max(0, natural * to - view));
}

/** `scale` as a percentage label: `125%`. */
export function zoomLabel(scale: number): string {
  return `${Math.round(scale * 100)}%`;
}
