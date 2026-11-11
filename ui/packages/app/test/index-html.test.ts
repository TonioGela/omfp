/** L1: the bundle's `index.html` (build.ts): import map, styles, the user theme last, and the favicon (UI-222). */

import { describe, expect, test } from "bun:test";
import { FAVICONS, indexHtml } from "../index-html.ts";

const html = indexHtml({
  importMap: { imports: { react: "/react-1.js" } },
  styles: ["/main-1.css"],
  main: "/main-1.js",
});
// A template's content is inert: happy-dom does not try to load the stylesheets.
const template = document.createElement("template");
template.innerHTML = html;
const head = template.content;

describe("index.html", () => {
  // spec: UI-222
  test("links the logo's π as the SVG favicon with a 32 px PNG fallback, both copied from docs/brand", () => {
    expect(
      [...head.querySelectorAll("link[rel=icon]")].map((link) => [
        link.getAttribute("href"),
        link.getAttribute("type"),
        link.getAttribute("sizes"),
      ]),
    ).toEqual([
      ["/icon-32.png", "image/png", "32x32"],
      ["/icon.svg", "image/svg+xml", "any"],
    ]);
    expect(FAVICONS.map(({ source, name }) => [source, name])).toEqual([
      ["docs/brand/icon-32.png", "icon-32.png"],
      ["docs/brand/icon.svg", "icon.svg"],
    ]);
  });

  // spec: UI-091
  test("the import map and the bundle's CSS come before the user theme, which wins", () => {
    expect([
      head.querySelector("script[type=importmap]")?.textContent,
      [...head.querySelectorAll("link[rel=stylesheet]")].map((link) => link.getAttribute("href")),
      head.querySelector("link[data-omfp-theme]")?.getAttribute("href"),
      html.includes('<script type="module" src="/main-1.js"></script>'),
    ]).toEqual(['{"imports":{"react":"/react-1.js"}}', ["/main-1.css", "/theme.css"], "/theme.css", true]);
  });
});
