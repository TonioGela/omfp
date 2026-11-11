/**
 * The test themes as role → color maps, read from the CSS the app ships (Synth, kit/src/tokens.css) and the gallery's
 * fixtures, the way a browser resolves them: the fixture's roles over Synth's, `ok`/`warn` mixed in OKLab (UI-136).
 */

import tokens from "../../../kit/src/tokens.css" with { type: "text" };
import dux from "../../src/gallery/dux-solarized.css" with { type: "text" };
import forest from "../../src/gallery/matugen-forest.css" with { type: "text" };
import { fromOklab, parseCssColor, type Rgb, toOklab } from "../../src/theme/contrast.ts";

export type Scheme = "light" | "dark";
export type ThemeName = "synth" | "matugen-forest" | "dux-solarized";

const DARK_BLOCK = ':root[data-color-scheme="dark"] {';

function roles(css: string, scheme: Scheme): Map<string, Rgb> {
  const block = scheme === "light" ? css.slice(0, css.indexOf("@media")) : css.slice(css.indexOf(DARK_BLOCK));
  const map = new Map<string, Rgb>();
  for (const [, name, value] of block.matchAll(/--omfp-([\w-]+):\s*(#[0-9a-fA-F]{6})/g)) {
    const color = parseCssColor(value ?? "");
    if (name !== undefined && color !== undefined) {
      map.set(name, color);
    }
  }
  return map;
}

function mixOklab(a: Rgb, b: Rgb, share: number): Rgb {
  const [x, y] = [toOklab(a), toOklab(b)];
  return fromOklab([
    x[0] * share + y[0] * (1 - share),
    x[1] * share + y[1] * (1 - share),
    x[2] * share + y[2] * (1 - share),
  ]);
}

const SOURCES: Record<ThemeName, string | null> = {
  synth: null,
  "matugen-forest": forest as unknown as string,
  "dux-solarized": dux as unknown as string,
};

/** The colors of `theme` in `scheme`, by token name without `--omfp-` (roles, `ok`, `warn`). */
export function themeColors(theme: ThemeName, scheme: Scheme): Map<string, Rgb> {
  const map = roles(tokens as unknown as string, scheme);
  const source = SOURCES[theme];
  if (source !== null) {
    for (const [name, color] of roles(source, scheme)) {
      map.set(name, color);
    }
  }
  const surface = map.get("surface");
  for (const [derived, role] of [
    ["ok", "success"],
    ["warn", "warning"],
  ] as const) {
    const color = map.get(role);
    if (color !== undefined && surface !== undefined) {
      map.set(derived, mixOklab(color, surface, 0.85));
    }
  }
  return map;
}

/** The text of a gallery fixture theme (`null`: Synth, no user theme). */
export function themeCss(theme: ThemeName): string | null {
  return SOURCES[theme];
}
