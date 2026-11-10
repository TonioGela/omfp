import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { basename, join } from "node:path";
import { cssFiles, KIT_SRC, parseCss, stripComments } from "./support/css.ts";

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
const tokensCss = readFileSync(join(KIT_SRC, "tokens.css"), "utf8");
const kitRules = parseCss(readFileSync(join(KIT_SRC, "kit.css"), "utf8"));

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

  // spec: UI-094
  test("serif only in prose, monospace only in code and logs, sans everywhere else", () => {
    const usages = kitRules.flatMap((rule) =>
      [...rule.declarations]
        .filter(([property]) => property === "font-family" || property === "font")
        .map(([, value]) => ({ selector: rule.selector, value })),
    );
    const misplaced = usages.filter(({ selector, value }) => {
      if (value.includes("--omfp-font-prose")) return !selector.includes(".omfp-prose");
      if (value.includes("--omfp-font-code")) return !/code|log|kbd|pre/.test(selector);
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
});
