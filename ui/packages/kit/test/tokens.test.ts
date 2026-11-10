import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { type CssRule, findRule, KIT_SRC, parseCss } from "./support/css.ts";
import { COLOR_TOKENS, DENSITY_TOKENS, FONT_TOKENS, prop, SHAPE_TOKENS, TEXT_PAIRS } from "./support/tokens.ts";

const LAYER = "@layer omfp-tokens";
const DARK_MEDIA = "@media (prefers-color-scheme: dark)";
const rules = parseCss(readFileSync(join(KIT_SRC, "tokens.css"), "utf8"));

const light = findRule(rules, [LAYER], ":root");
const darkBySystem = findRule(rules, [LAYER, DARK_MEDIA], ':root:not([data-color-scheme="light"])');
const darkForced = findRule(rules, [LAYER], ':root[data-color-scheme="dark"]');
const compact = findRule(rules, [LAYER], ':root, :root[data-density="compact"]');
const normal = findRule(rules, [LAYER], ':root[data-density="normal"]');

function missing(rule: CssRule, tokens: readonly string[]): string[] {
  return tokens.filter((token) => !rule.declarations.has(prop(token)));
}

function value(rule: CssRule, token: string): string {
  const found = rule.declarations.get(prop(token));
  if (found === undefined) throw new Error(`${prop(token)} missing in ${rule.selector}`);
  return found;
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
  // spec: UI-091
  test("every rule lives in the omfp-tokens layer", () => {
    expect(rules.filter((rule) => rule.at[0] !== LAYER)).toEqual([]);
  });

  // spec: UI-091
  test("light (:root) defines every color, font, density and shape token", () => {
    expect(missing(light, [...COLOR_TOKENS, ...FONT_TOKENS, ...SHAPE_TOKENS])).toEqual([]);
    expect(light.declarations.get("color-scheme")).toBe("light");
  });

  // spec: UI-091
  test("dark follows prefers-color-scheme and the data-color-scheme override with the same colors", () => {
    expect(missing(darkBySystem, COLOR_TOKENS)).toEqual([]);
    expect(missing(darkForced, COLOR_TOKENS)).toEqual([]);
    expect([...darkBySystem.declarations]).toEqual([...darkForced.declarations]);
    expect(darkForced.declarations.get("color-scheme")).toBe("dark");
    expect(COLOR_TOKENS.filter((token) => value(darkForced, token) === value(light, token))).toEqual([
      "shadow",
      "scrim",
    ]);
  });

  // spec: UI-091
  test.each(["light", "dark"] as const)("text roles reach 4.5:1 contrast in %s", (scheme) => {
    const rule = scheme === "light" ? light : darkForced;
    const failing = TEXT_PAIRS.map(([text, background]) => ({
      pair: `${text} on ${background}`,
      ratio: contrast(value(rule, text), value(rule, background)),
    })).filter(({ ratio }) => ratio < 4.5);
    expect(failing).toEqual([]);
  });

  // spec: UI-094
  test("fonts default to the system families: serif prose, sans interface, monospace code", () => {
    expect(value(light, "font-prose")).toStartWith("ui-serif,");
    expect(value(light, "font-prose")).toEndWith(", serif");
    expect(value(light, "font-ui")).toStartWith("system-ui,");
    expect(value(light, "font-ui")).toEndWith(", sans-serif");
    expect(value(light, "font-code")).toStartWith("ui-monospace,");
    expect(value(light, "font-code")).toEndWith(", monospace");
  });

  // spec: UI-094
  test("density is compact by default and normal on data-density", () => {
    expect(missing(compact, DENSITY_TOKENS)).toEqual([]);
    expect(missing(normal, DENSITY_TOKENS)).toEqual([]);
    expect(Number.parseFloat(value(normal, "row-height"))).toBeGreaterThan(
      Number.parseFloat(value(compact, "row-height")),
    );
  });
});
