/** L1: the `/kit` gallery's parameters (UI-173) and its WCAG contrast arithmetic (UI-174, UI-135). */

import { describe, expect, test } from "bun:test";
import { contrastRatio, failingPairs, parseRgb, relativeLuminance } from "../../src/gallery/contrast.ts";
import { galleryParams, isGalleryPath } from "../../src/gallery/params.ts";

describe("gallery parameters", () => {
  // spec: UI-173
  test("the page is served at /kit", () => {
    expect(isGalleryPath("/kit")).toBe(true);
    expect(isGalleryPath("/kit/")).toBe(true);
    expect(isGalleryPath("/")).toBe(false);
    expect(isGalleryPath("/kits")).toBe(false);
  });

  // spec: UI-173
  test("scheme, theme, density, motion and font size come from the query, with the app's defaults", () => {
    expect(galleryParams("")).toEqual({
      scheme: "system",
      theme: "synth",
      density: "compact",
      motion: "full",
      fontSize: 17,
    });
    expect(galleryParams("?scheme=dark&theme=matugen-forest&density=normal&motion=none&fs=15")).toEqual({
      scheme: "dark",
      theme: "matugen-forest",
      density: "normal",
      motion: "none",
      fontSize: 15,
    });
  });

  // spec: UI-173
  test("unknown values fall back to the defaults", () => {
    expect(galleryParams("?scheme=sepia&theme=nope&density=huge&motion=fast&fs=abc")).toEqual(galleryParams(""));
    expect(galleryParams("?fs=4").fontSize).toBe(17);
  });
});

describe("contrast", () => {
  // spec: UI-174
  test("computed colors parse to sRGB channels", () => {
    expect(parseRgb("rgb(0, 104, 118)")).toEqual([0, 104, 118]);
    expect(parseRgb("rgba(250, 248, 255, 1)")).toEqual([250, 248, 255]);
    expect(parseRgb("oklab(0.5 0 0)")).toBeUndefined();
  });

  // spec: UI-174
  test("WCAG relative luminance and contrast ratio", () => {
    expect(relativeLuminance([255, 255, 255])).toBeCloseTo(1, 5);
    expect(relativeLuminance([0, 0, 0])).toBe(0);
    expect(contrastRatio([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 5);
    expect(contrastRatio([255, 255, 255], [0, 0, 0])).toBeCloseTo(21, 5);
    // Synth light: primary on surface.
    expect(contrastRatio([0, 0x68, 0x76], [0xfa, 0xf8, 0xff])).toBeCloseTo(6.15, 1);
  });

  // spec: UI-174, UI-135
  test("pairs under AA (4.5:1) are reported", () => {
    expect(
      failingPairs([
        { text: "primary", background: "surface", ratio: 6.3 },
        { text: "outline", background: "surface", ratio: 4.2 },
      ]),
    ).toEqual(["outline on surface: 4.20"]);
  });
});
