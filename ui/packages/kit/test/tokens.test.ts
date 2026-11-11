import { describe, expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { type CssRule, findRule, KIT_SRC, parseCss } from "./support/css.ts";
import {
  COLOR_TOKENS,
  DENSITY_SIZES,
  DERIVED_TOKENS,
  FONT_FILES,
  FONT_TOKENS,
  FRAME_SIZES,
  prop,
  SHAPE_TOKENS,
  SPACE_STEPS,
  SURFACE_TOKENS,
  SYNTH,
  TEXT_PAIRS,
  TYPE_SCALE,
} from "./support/tokens.ts";

const LAYER = "@layer omfp-tokens";
const DARK_MEDIA = "@media (prefers-color-scheme: dark)";
const TOKENS_CSS = join(KIT_SRC, "tokens.css");
const rules = parseCss(readFileSync(TOKENS_CSS, "utf8"));

const light = findRule(rules, [LAYER], ":root");
const darkBySystem = findRule(rules, [LAYER, DARK_MEDIA], ':root:not([data-color-scheme="light"])');
const darkForced = findRule(rules, [LAYER], ':root[data-color-scheme="dark"]');
const normal = findRule(rules, [LAYER], ':root[data-density="normal"]');
const fontFaces = rules.filter((rule) => rule.selector === "@font-face");

function missing(rule: CssRule, tokens: readonly string[]): string[] {
  return tokens.filter((token) => !rule.declarations.has(prop(token)));
}

/** Whitespace inside parentheses does not matter (multi-line values). */
function tidy(text: string): string {
  return text.replace(/\(\s+/g, "(").replace(/\s+\)/g, ")");
}

function value(rule: CssRule, token: string): string {
  const found = rule.declarations.get(prop(token));
  if (found === undefined) throw new Error(`${prop(token)} missing in ${rule.selector}`);
  return tidy(found);
}

/** The tokens of `expected` whose value in `rule` differs, with the value found. */
function differing(rule: CssRule, expected: Readonly<Record<string, string>>): Record<string, string | undefined> {
  return Object.fromEntries(
    Object.entries(expected)
      .filter(([token, wanted]) => {
        const found = rule.declarations.get(prop(token));
        return found === undefined || tidy(found) !== wanted;
      })
      .map(([token]) => [token, rule.declarations.get(prop(token))]),
  );
}

/** WCAG relative luminance of a `#rrggbb` color. */
function luminance(hex: string): number {
  const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
  if (match === null) throw new Error(`not a #rrggbb color: ${hex}`);
  const [r, g, b] = match.slice(1).map((channel) => {
    const c = Number.parseInt(channel, 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0);
}

function contrast(a: string, b: string): number {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return ((high ?? 0) + 0.05) / ((low ?? 0) + 0.05);
}

describe("tokens.css", () => {
  // spec: UI-091, UI-125
  test("every rule lives in the omfp-tokens layer", () => {
    expect(rules.filter((rule) => rule.at[0] !== LAYER)).toEqual([]);
  });

  // spec: UI-091, UI-125
  test("light (:root) defines every color and font token", () => {
    expect(missing(light, [...COLOR_TOKENS, ...FONT_TOKENS])).toEqual([]);
    expect(light.declarations.get("color-scheme")).toBe("light");
  });

  // spec: UI-139
  test("the built-in palette is Synth, light on :root", () => {
    expect(differing(light, Object.fromEntries(COLOR_TOKENS.map((token) => [token, SYNTH[token]?.[0] ?? ""])))).toEqual(
      {},
    );
  });

  // spec: UI-091, UI-139
  test("dark is Synth dark, by prefers-color-scheme and by the data-color-scheme override alike", () => {
    expect(
      differing(darkForced, Object.fromEntries(COLOR_TOKENS.map((token) => [token, SYNTH[token]?.[1] ?? ""]))),
    ).toEqual({});
    expect([...darkBySystem.declarations]).toEqual([...darkForced.declarations]);
    expect(darkForced.declarations.get("color-scheme")).toBe("dark");
  });

  // spec: UI-091, UI-139
  test.each(["light", "dark"] as const)("text roles reach 4.5:1 contrast in %s", (scheme) => {
    const rule = scheme === "light" ? light : darkForced;
    const failing = TEXT_PAIRS.map(([text, background]) => ({
      pair: `${text} on ${background}`,
      ratio: contrast(value(rule, text), value(rule, background)),
    })).filter(({ ratio }) => ratio < 4.5);
    expect(failing).toEqual([]);
  });

  // spec: UI-121, UI-142
  test("prose is IBM Plex Serif, code Commit Mono, and the interface follows the code family", () => {
    expect(value(light, "font-prose")).toBe('"IBM Plex Serif", ui-serif, Georgia, serif');
    expect(value(light, "font-code")).toBe('"Commit Mono", ui-monospace, monospace');
    expect(value(light, "font-ui")).toBe("var(--omfp-font-code)");
  });

  // spec: UI-141, UI-142
  test("one @font-face per bundled file, swapping, with the file's weight and style; the files and licences ship", () => {
    expect(
      fontFaces.map((rule) => [
        rule.declarations.get("font-family"),
        rule.declarations.get("src"),
        rule.declarations.get("font-weight"),
        rule.declarations.get("font-style"),
        rule.declarations.get("font-display"),
      ]),
    ).toEqual(
      FONT_FILES.map(([family, stem, weight, style]) => [
        `"${family}"`,
        `url("/fonts/${stem}.woff2") format("woff2")`,
        weight,
        style,
        "swap",
      ]),
    );
    const fonts = join(KIT_SRC, "..", "fonts");
    expect(FONT_FILES.filter(([, stem]) => !existsSync(join(fonts, `${stem}.woff2`)))).toEqual([]);
    expect(
      ["ibm-plex-serif-OFL.txt", "commit-mono-OFL.txt"].filter(
        (licence) => !readFileSync(join(fonts, licence), "utf8").includes("SIL Open Font License, Version 1.1"),
      ),
    ).toEqual([]);
  });

  // spec: UI-126
  test("the type scale, weights and line heights are fixed rem values, the same in both densities", () => {
    expect(differing(light, TYPE_SCALE)).toEqual({});
  });

  // spec: UI-127, UI-094
  test("spacing steps are unit × n × density, compact by default and normal on data-density", () => {
    expect(value(light, "unit")).toBe("0.25rem");
    expect(value(light, "density")).toBe("1");
    expect(value(normal, "density")).toBe("1.3");
    expect([...normal.declarations.keys()]).toEqual([prop("density")]);
    expect(
      differing(
        light,
        Object.fromEntries(
          SPACE_STEPS.map((n) => [`s${n}`, `calc(var(--omfp-unit) * ${n} * var(--omfp-density))`] as const),
        ),
      ),
    ).toEqual({});
  });

  // spec: UI-128
  test("row, control and message-gap sizes scale with density; the frame sizes do not", () => {
    expect(
      differing(
        light,
        Object.fromEntries(
          Object.entries(DENSITY_SIZES).map(([token, size]) => [token, `calc(${size} * var(--omfp-density))`]),
        ),
      ),
    ).toEqual({});
    expect(differing(light, FRAME_SIZES)).toEqual({});
  });

  // spec: UI-129, UI-130, UI-131, UI-132, UI-133
  test("radii, rules, surfaces, icons, focus and interactive states", () => {
    expect(differing(light, SHAPE_TOKENS)).toEqual({});
    expect(differing(light, SURFACE_TOKENS)).toEqual({});
    expect(differing(darkForced, { "pop-bg": "var(--omfp-surface-container)" })).toEqual({});
  });

  // spec: UI-134, UI-135, UI-136
  test("gradients, ok and warn derive from the roles, with the end stop pulled toward on-surface", () => {
    expect(differing(light, DERIVED_TOKENS)).toEqual({});
    const derived = Object.keys(DERIVED_TOKENS).map(prop);
    expect([...darkForced.declarations.keys()].filter((name) => derived.includes(name))).toEqual([]);
  });
});
