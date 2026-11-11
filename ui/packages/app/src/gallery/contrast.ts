/**
 * WCAG 2 contrast (UI-174, UI-135): the gallery measures its text roles and gradient stops with it, and the
 * screenshot test reads the results from the page.
 */

export type Rgb = readonly [red: number, green: number, blue: number];

/** One measured pair: a text color on a background. */
export interface ContrastPair {
  readonly text: string;
  readonly background: string;
  readonly ratio: number;
}

/** AA for normal text. */
export const AA = 4.5;

/** The channels of a computed `rgb()`/`rgba()` color; `undefined` for any other notation. */
export function parseRgb(color: string): Rgb | undefined {
  const match = /^rgba?\(\s*(\d+(?:\.\d+)?),\s*(\d+(?:\.\d+)?),\s*(\d+(?:\.\d+)?)/.exec(color.trim());
  if (match === null) {
    return undefined;
  }
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

/** WCAG relative luminance of an sRGB color (channels 0–255). */
export function relativeLuminance([red, green, blue]: Rgb): number {
  const linear = (channel: number) => {
    const c = channel / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue);
}

/** The contrast ratio of two colors, 1 to 21, whichever is lighter. */
export function contrastRatio(a: Rgb, b: Rgb): number {
  const [high, low] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x);
  return ((high ?? 0) + 0.05) / ((low ?? 0) + 0.05);
}

/** The pairs under AA, as `text on background: ratio`. */
export function failingPairs(pairs: readonly ContrastPair[]): string[] {
  return pairs
    .filter((pair) => pair.ratio < AA)
    .map((pair) => `${pair.text} on ${pair.background}: ${pair.ratio.toFixed(2)}`);
}
