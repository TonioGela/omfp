/** L4: the logo (UI-220, UI-221): the generated outlines and files, the brand's wordmark and the strip's mark. */

import { afterEach, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cleanup, render, screen } from "@testing-library/react";
import { Brand, LogoIcon, LogoMark, ThreadStrip, Wordmark } from "../src/index.ts";
import { ICON_PI, MARK_BRACKETS, MARK_PI, WORDMARK_PI, WORDMARK_TEXT } from "../src/logo-paths.ts";

const BRAND = join(import.meta.dir, "../../../../docs/brand");
const FIXED_STOPS = [
  ["0", "#16b3cc"],
  ["0.62", "#6a5cf0"],
  ["1", "#8a6cf0"],
];
const THEME_STOPS = [
  ["0", "var(--omfp-primary)"],
  ["0.3", "var(--omfp-tertiary)"],
  ["0.5", "var(--omfp-grad-end)"],
  ["0.7", "var(--omfp-tertiary)"],
  ["1", "var(--omfp-primary)"],
];

afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.motion;
});

function brandFile(name: string): Document {
  return new DOMParser().parseFromString(readFileSync(join(BRAND, name), "utf8"), "image/svg+xml");
}

/** Each path of an SVG: its outline, fill and opacity. */
function paths(root: ParentNode): (string | null)[][] {
  return [...root.querySelectorAll("path")].map((path) => [
    path.getAttribute("d"),
    path.getAttribute("fill"),
    path.getAttribute("opacity"),
  ]);
}

/** The stops of the gradient a path is filled with: offset and color (attribute or style). */
function gradientStops(root: ParentNode, path: Element | null | undefined): (string | null)[][] {
  const id = /^url\(#(.+)\)$/.exec(path?.getAttribute("fill") ?? "")?.[1];
  const gradient = id === undefined ? null : root.querySelector(`linearGradient[id="${id}"]`);
  return [...(gradient?.querySelectorAll("stop") ?? [])].map((stop) => [
    stop.getAttribute("offset"),
    stop.getAttribute("stop-color") ?? (stop as SVGStopElement).style.stopColor,
  ]);
}

describe("logo files", () => {
  // spec: UI-220
  test("the wordmark files draw the kit's outlines: omf in the text colour, π in the fixed gradient", () => {
    for (const [file, text] of [
      ["wordmark.svg", "currentColor"],
      ["wordmark-light.svg", "#1a1b23"],
      ["wordmark-dark.svg", "#e2e1ec"],
    ] as const) {
      const svg = brandFile(file);
      expect([file, paths(svg)]).toEqual([
        file,
        [...WORDMARK_TEXT.map((d) => [d, text, null]), [WORDMARK_PI, "url(#omfp-grad)", null]],
      ]);
      expect([file, gradientStops(svg, svg.querySelectorAll("path")[3])]).toEqual([file, FIXED_STOPS]);
    }
  });

  // spec: UI-220
  test("the mark files draw muted brackets and the gradient π; the icon only the π", () => {
    for (const [file, text] of [
      ["mark.svg", "currentColor"],
      ["mark-light.svg", "#1a1b23"],
      ["mark-dark.svg", "#e2e1ec"],
    ] as const) {
      const svg = brandFile(file);
      expect([file, paths(svg)]).toEqual([
        file,
        [...MARK_BRACKETS.map((d) => [d, text, "0.62"]), [MARK_PI, "url(#omfp-grad)", null]],
      ]);
      expect([file, gradientStops(svg, svg.querySelectorAll("path")[2])]).toEqual([file, FIXED_STOPS]);
    }
    const icon = brandFile("icon.svg");
    expect(paths(icon)).toEqual([[ICON_PI, "url(#omfp-grad)", null]]);
    expect(gradientStops(icon, icon.querySelector("path"))).toEqual(FIXED_STOPS);
  });
});

describe("logo components", () => {
  // spec: UI-221, UI-170
  test("the brand is the wordmark, π filled with the theme's gradient, then @host", () => {
    render(<Brand host="devbox" data-testid="brand" />);
    const brand = screen.getByTestId("brand");
    const wordmark = screen.getByRole("img", { name: "omfp" });
    expect([brand.textContent, wordmark.getAttribute("class"), wordmark.dataset.gradient]).toEqual([
      "@devbox",
      "omfp-logo omfp-brand__mark",
      "theme",
    ]);
    expect(paths(wordmark).map(([d, fill]) => [d, fill?.startsWith("url(#") ? "gradient" : fill])).toEqual([
      ...WORDMARK_TEXT.map((d) => [d, "currentColor"]),
      [WORDMARK_PI, "gradient"],
    ]);
    expect(gradientStops(wordmark, wordmark.querySelectorAll("path")[3])).toEqual(THEME_STOPS);
  });

  // spec: UI-221
  test("each logo has its own gradient id", () => {
    render(
      <>
        <Wordmark data-testid="logo" />
        <LogoMark data-testid="logo" />
        <LogoIcon data-testid="logo" />
      </>,
    );
    const ids = screen.getAllByTestId("logo").map((svg) => svg.querySelector("linearGradient")?.id ?? "");
    expect([ids.length, new Set(ids).size, ids.every((id) => /^[\w-]+$/.test(id))]).toEqual([3, 3, true]);
  });

  // spec: UI-221, UI-145
  test("the theme gradient flows at motion full only", () => {
    const flows = (motion: string): number => {
      document.documentElement.dataset.motion = motion;
      const { container, unmount } = render(<Wordmark />);
      const count = container.querySelectorAll("linearGradient animateTransform").length;
      unmount();
      return count;
    };
    expect([flows("full"), flows("reduced"), flows("none")]).toEqual([1, 0, 0]);
  });

  // spec: UI-221
  test("the file gradient variant uses the files' fixed stops; the icon is the π alone", () => {
    render(<LogoIcon gradient="file" data-testid="icon" />);
    const icon = screen.getByTestId("icon");
    expect([icon.dataset.gradient, paths(icon).map(([d]) => d)]).toEqual(["file", [ICON_PI]]);
    expect(gradientStops(icon, icon.querySelector("path"))).toEqual(FIXED_STOPS);
  });

  // spec: UI-221, UI-208, UI-201
  test("the strip shows the mark: muted brackets and the theme-gradient π, hidden from assistive tech", () => {
    render(
      <ThreadStrip
        open={false}
        onToggle={() => undefined}
        toggleLabel="Show threads"
        threads={[]}
        onSelect={() => undefined}
        data-testid="strip"
      />,
    );
    const mark = screen.getByTestId("strip").querySelector(".omfp-strip__brand");
    expect([mark?.tagName.toLowerCase(), mark?.getAttribute("aria-hidden")]).toEqual(["svg", "true"]);
    expect(paths(mark ?? document).map(([d, fill, opacity]) => [d, fill?.slice(0, 5), opacity])).toEqual([
      ...MARK_BRACKETS.map((d) => [d, "curre", "0.62"]),
      [MARK_PI, "url(#", null],
    ]);
    expect(gradientStops(mark ?? document, mark?.querySelectorAll("path")[2])).toEqual(THEME_STOPS);
  });
});
