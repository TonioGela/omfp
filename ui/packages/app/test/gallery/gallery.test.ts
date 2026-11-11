/** L1: the `/kit` gallery's parameters (UI-173). */

import { describe, expect, test } from "bun:test";
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

  // spec: UI-173, UI-196
  test("the Dux preset fixture is a gallery theme", () => {
    expect(galleryParams("?theme=dux-solarized").theme).toBe("dux-solarized");
  });
});
