import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { cssFiles, findRule, KIT_SRC, parseCss, stripComments } from "./support/css.ts";
import { RETIRED_TOKEN } from "./support/tokens.ts";

/** CSS named and system colors (`transparent` and `currentColor` are allowed: they carry no palette choice). */
const NAMED_COLORS = new Set(
  `aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue blueviolet brown burlywood
  cadetblue chartreuse chocolate coral cornflowerblue cornsilk crimson cyan darkblue darkcyan darkgoldenrod darkgray
  darkgreen darkgrey darkkhaki darkmagenta darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen
  darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink deepskyblue dimgray dimgrey dodgerblue
  firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite gold goldenrod gray green greenyellow grey honeydew
  hotpink indianred indigo ivory khaki lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan
  lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen lightskyblue lightslategray
  lightslategrey lightsteelblue lightyellow lime limegreen linen magenta maroon mediumaquamarine mediumblue
  mediumorchid mediumpurple mediumseagreen mediumslateblue mediumspringgreen mediumturquoise mediumvioletred
  midnightblue mintcream mistyrose moccasin navajowhite navy oldlace olive olivedrab orange orangered orchid
  palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum powderblue purple
  rebeccapurple red rosybrown royalblue saddlebrown salmon sandybrown seagreen seashell sienna silver skyblue slateblue
  slategray slategrey snow springgreen steelblue tan teal thistle tomato turquoise violet wheat white whitesmoke yellow
  yellowgreen accentcolor accentcolortext activetext buttonborder buttonface buttontext canvas canvastext field
  fieldtext graytext highlight highlighttext linktext mark marktext selecteditem selecteditemtext visitedtext`.split(
    /\s+/,
  ),
);

const HEX_COLOR = /#[0-9a-f]{3,8}\b/gi;
const COLOR_FUNCTION = /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(/gi;
const VAR_REF = /var\(\s*(--[\w-]+)/g;

const styleFiles = cssFiles(KIT_SRC).filter((path) => basename(path) !== "tokens.css");
const APP_SRC = join(KIT_SRC, "..", "..", "app", "src");
const tokensCss = readFileSync(join(KIT_SRC, "tokens.css"), "utf8");
const kitRules = parseCss(readFileSync(join(KIT_SRC, "kit.css"), "utf8"));
const LAYER = "@layer omfp-kit";

/** Color literals of one stylesheet: hex colors, color functions and named colors in declaration values. */
function colorLiterals(css: string): string[] {
  const text = stripComments(css);
  const literals = [...(text.match(HEX_COLOR) ?? []), ...(text.match(COLOR_FUNCTION) ?? [])];
  for (const rule of parseCss(css)) {
    for (const [property, value] of rule.declarations) {
      const words = value.replace(VAR_REF, "var(").match(/-?[a-z][\w-]*/gi) ?? [];
      literals.push(...words.filter((word) => NAMED_COLORS.has(word.toLowerCase())).map((w) => `${property}: ${w}`));
    }
  }
  return literals;
}

function definedTokens(css: string): Set<string> {
  return new Set(parseCss(css).flatMap((rule) => [...rule.declarations.keys()].filter((k) => k.startsWith("--omfp-"))));
}

describe("kit styles", () => {
  test("the kit ships its component stylesheet", () => {
    expect(styleFiles.map((path) => basename(path))).toContain("kit.css");
  });

  // spec: UI-091
  test.each(styleFiles.map((path) => [basename(path), path] as const))("%s has no color literals", (_name, path) => {
    expect(colorLiterals(readFileSync(path, "utf8"))).toEqual([]);
  });

  // spec: UI-091
  test("the scanner catches every kind of color literal", () => {
    const sample = `.a { color: #fff; background: rgb(0 0 0); border-color: hsl(1 2% 3%); fill: RebeccaPurple; }
      .b { outline-color: Canvas; color: color-mix(in srgb, var(--omfp-primary) 50%, transparent); }`;
    expect(colorLiterals(sample)).toEqual(["#fff", "rgb(", "hsl(", "fill: RebeccaPurple", "outline-color: Canvas"]);
  });

  // spec: UI-091
  test("kit.css only uses tokens that tokens.css defines", () => {
    const defined = definedTokens(tokensCss);
    const css = stripComments(readFileSync(join(KIT_SRC, "kit.css"), "utf8"));
    const used = new Set([...css.matchAll(VAR_REF)].map((match) => match[1] ?? ""));
    expect([...used].filter((name) => name.startsWith("--omfp-") && !defined.has(name))).toEqual([]);
  });

  // spec: UI-125
  test("the app's stylesheets only use tokens that tokens.css defines", () => {
    const defined = definedTokens(tokensCss);
    const undefinedTokens = cssFiles(APP_SRC).flatMap((path) =>
      [...stripComments(readFileSync(path, "utf8")).matchAll(VAR_REF)]
        .map((match) => match[1] ?? "")
        .filter((name) => name.startsWith("--omfp-") && !defined.has(name))
        .map((name) => `${basename(path)}: ${name}`),
    );
    expect(undefinedTokens).toEqual([]);
  });

  // spec: UI-125
  test("the earlier size, radius, line-height and spacing tokens are gone everywhere, with no aliases", () => {
    const sources = [KIT_SRC, APP_SRC].flatMap((dir) =>
      readdirSync(dir, { recursive: true, encoding: "utf8" })
        .filter((name) => /\.(?:css|tsx?)$/.test(name))
        .map((name) => join(dir, name)),
    );
    expect(sources.filter((path) => RETIRED_TOKEN.test(readFileSync(path, "utf8")))).toEqual([]);
  });

  // spec: UI-148
  test("key chips: code family at fs-2xs, optically centred on the text baseline, tertiary on surface-container", () => {
    const chip = findRule(kitRules, [LAYER], ".omfp-kbd");
    expect(Object.fromEntries(chip.declarations)).toEqual({
      display: "inline-block",
      "min-inline-size": "1.6em",
      padding: "0.36em 0.4em",
      "font-family": "var(--omfp-font-code)",
      "font-size": "var(--omfp-fs-2xs)",
      "line-height": "1",
      "text-box": "trim-both ex alphabetic",
      "text-align": "center",
      "white-space": "nowrap",
      color: "var(--omfp-tertiary)",
      border: "var(--omfp-bw) solid color-mix(in oklab, var(--omfp-tertiary) 40%, transparent)",
      "border-radius": "var(--omfp-r-chip)",
      background: "var(--omfp-kbd-bg)",
    });
    const inButton = findRule(kitRules, [LAYER], ".omfp-button .omfp-kbd");
    expect(Object.fromEntries(inButton.declarations)).toEqual({ color: "inherit", background: "transparent" });
  });

  // spec: UI-145
  test("motion none stops every animation and transition and makes scrolling instant; the effective level is data-motion", () => {
    const none = findRule(
      kitRules,
      [LAYER],
      ':root[data-motion="none"], :root[data-motion="none"] *, :root[data-motion="none"] *::before, :root[data-motion="none"] *::after',
    );
    expect(Object.fromEntries(none.declarations)).toEqual({
      animation: "none !important",
      transition: "none !important",
      "scroll-behavior": "auto !important",
    });
    const css = stripComments(readFileSync(join(KIT_SRC, "kit.css"), "utf8"));
    expect(css).not.toContain("prefers-reduced-motion");
  });

  // spec: UI-094, UI-121
  test("the prose family only in prose; chrome in the ui family (which is the code family), code in code", () => {
    const usages = kitRules.flatMap((rule) =>
      [...rule.declarations]
        .filter(([property]) => property === "font-family" || property === "font")
        .map(([, value]) => ({ selector: rule.selector, value })),
    );
    // Code (UI-121): code blocks, inline code, logs, key chips, diffs, mono fields (paths, commands), the command
    // line, tool arguments, ask option labels and completed paths.
    const misplaced = usages.filter(({ selector, value }) => {
      if (value.includes("--omfp-font-prose")) return !selector.includes(".omfp-prose");
      if (value.includes("--omfp-font-code"))
        return !/code|log|kbd|pre|diff|mono|cmd|__arg|option-text__label|path-field__name/.test(selector);
      return !(value.includes("--omfp-font-ui") || value === "inherit");
    });
    expect(misplaced).toEqual([]);
    expect(usages.some(({ value }) => value.includes("--omfp-font-prose"))).toBe(true);
    expect(usages.some(({ value }) => value.includes("--omfp-font-code"))).toBe(true);
  });

  // spec: UI-091
  test("component styles are layered after the tokens", () => {
    const css = stripComments(readFileSync(join(KIT_SRC, "kit.css"), "utf8"));
    expect(css).toContain("@layer omfp-tokens, omfp-kit;");
    expect(kitRules.filter((rule) => rule.at[0] !== "@layer omfp-kit")).toEqual([]);
  });

  // spec: UI-123
  test("gradients only where listed", () => {
    const GRADIENT = /var\(--omfp-grad(?:-loop)?\)/;
    const ALLOWED =
      /grad-text|grad-flow|data-variant="primary"|tabs__tab\[data-selected\]::after|mode-pill|cmd__prompt|omfp-caret|progress__fill|gauge__fill|compose\[data-mode="insert"\]|tool\[data-status="running"\]|@keyframes/;
    const misplaced = kitRules
      .filter((rule) => [...rule.declarations.values()].some((value) => GRADIENT.test(value)))
      .map((rule) => rule.selector)
      .filter((selector) => !ALLOWED.test(selector));
    expect(misplaced).toEqual([]);
  });

  // spec: UI-138
  test("diff lines tint with ok and error; syntax strings are ok, numbers warn, comments muted italic", () => {
    expect(Object.fromEntries(findRule(kitRules, [LAYER], '.omfp-diff__line[data-kind="add"]').declarations)).toEqual({
      color: "color-mix(in oklab, var(--omfp-ok) 70%, var(--omfp-on-surface))",
      background: "color-mix(in oklab, var(--omfp-ok) 12%, transparent)",
    });
    expect(Object.fromEntries(findRule(kitRules, [LAYER], '.omfp-diff__line[data-kind="del"]').declarations)).toEqual({
      color: "color-mix(in oklab, var(--omfp-error) 70%, var(--omfp-on-surface))",
      background: "color-mix(in oklab, var(--omfp-error) 11%, transparent)",
    });
    const color = (selector: string) => findRule(kitRules, [LAYER], selector).declarations.get("color");
    expect(
      color(".omfp-code :is(.hljs-string, .hljs-regexp, .hljs-subst, .hljs-template-variable, .hljs-addition)"),
    ).toBe("var(--omfp-ok)");
    expect(
      color(
        ".omfp-code :is(.hljs-number, .hljs-literal, .hljs-symbol, .hljs-bullet, .hljs-variable.constant_, .hljs-meta)",
      ),
    ).toBe("var(--omfp-warn)");
    expect(color(".omfp-code :is(.hljs-comment, .hljs-quote)")).toBe("var(--omfp-on-surface-variant)");
    expect(findRule(kitRules, [LAYER], "::selection").declarations.get("background")).toBe(
      "color-mix(in oklab, var(--omfp-primary) 28%, transparent)",
    );
  });

  // spec: UI-147, UI-146
  test("the stream caret blinks at motion full only; elsewhere it is steady", () => {
    const blinking = kitRules.filter(
      (rule) => rule.selector.includes(".omfp-caret") && rule.declarations.has("animation"),
    );
    expect(blinking.map((rule) => [rule.selector, rule.declarations.get("animation")])).toEqual([
      [':root[data-motion="full"] .omfp-caret', "omfp-blink 0.9s steps(1) infinite"],
    ]);
  });
});
