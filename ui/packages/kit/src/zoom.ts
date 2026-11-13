/**
 * Zoom of the kit's document viewers (`PdfView`, UI-544): scales as fractions of the actual size (1 = 100%), the steps
 * the zoom buttons go through, and the scale that fits a width.
 */

/** The scales the zoom buttons step through, smallest first. */
export const ZOOM_STEPS: readonly number[] = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

const MIN_ZOOM = 0.25;
const MAX_ZOOM = 4;
/** Two scales this close are the same step (a fitted scale near a step goes to the next one). */
const SAME = 0.005;

/** The next step above `scale`, or the largest. */
export function zoomIn(scale: number): number {
  return ZOOM_STEPS.find((step) => step > scale + SAME) ?? MAX_ZOOM;
}

/** The next step below `scale`, or the smallest. */
export function zoomOut(scale: number): number {
  return ZOOM_STEPS.findLast((step) => step < scale - SAME) ?? MIN_ZOOM;
}

/** The scale at which something `width` wide at 100% fills `available` pixels, within the zoom's bounds. */
export function fitScale(available: number, width: number): number {
  if (!(available > 0 && width > 0)) {
    return 1;
  }
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, available / width));
}

/** `scale` as a percentage label: `125%`. */
export function zoomLabel(scale: number): string {
  return `${Math.round(scale * 100)}%`;
}
