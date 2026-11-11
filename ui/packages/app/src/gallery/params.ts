/**
 * The `/kit` gallery's parameters (UI-173): `?scheme=light|dark`, `?theme=synth|matugen-forest`, `?density=`,
 * `?motion=` and `?fs=` (the root size in px). Unknown values keep the app's defaults.
 */
import type { Density, Motion } from "@omfp/protocol";
import { type ColorScheme, DEFAULT_DENSITY, DEFAULT_FONT_SIZE, DEFAULT_MOTION } from "../sdk/index.ts";

/** The built-in palette, or the matugen fixture theme. */
export type GalleryTheme = "synth" | "matugen-forest";

export interface GalleryParams {
  readonly scheme: ColorScheme;
  readonly theme: GalleryTheme;
  readonly density: Density;
  readonly motion: Motion;
  readonly fontSize: number;
}

const SCHEMES: readonly ColorScheme[] = ["system", "light", "dark"];
const THEMES: readonly GalleryTheme[] = ["synth", "matugen-forest"];
const DENSITIES: readonly Density[] = ["compact", "normal"];
const MOTIONS: readonly Motion[] = ["full", "reduced", "none"];
/** Root sizes the gallery accepts (px), the `[ui] font_size` range. */
const MIN_FONT_SIZE = 10;
const MAX_FONT_SIZE = 32;

function pick<T extends string>(value: string | null, allowed: readonly T[], fallback: T): T {
  return allowed.find((candidate) => candidate === value) ?? fallback;
}

/** Whether `pathname` is the gallery's. */
export function isGalleryPath(pathname: string): boolean {
  return pathname === "/kit" || pathname === "/kit/";
}

/** The parameters of a gallery URL's query string. */
export function galleryParams(search: string): GalleryParams {
  const query = new URLSearchParams(search);
  const size = Number(query.get("fs"));
  return {
    scheme: pick(query.get("scheme"), SCHEMES, "system"),
    theme: pick(query.get("theme"), THEMES, "synth"),
    density: pick(query.get("density"), DENSITIES, DEFAULT_DENSITY),
    motion: pick(query.get("motion"), MOTIONS, DEFAULT_MOTION),
    fontSize: Number.isInteger(size) && size >= MIN_FONT_SIZE && size <= MAX_FONT_SIZE ? size : DEFAULT_FONT_SIZE,
  };
}
