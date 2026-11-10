import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { type CssRule, findRule, MATUGEN_DIR, parseCss } from "./support/css.ts";
import { COLOR_TOKENS, prop } from "./support/tokens.ts";

const PLACEHOLDER = /\{\{\s*([\w.]+)\s*\}\}/g;
const template = readFileSync(join(MATUGEN_DIR, "omfp.css"), "utf8");
// matugen's `{{ … }}` placeholders would read as blocks: parse them as plain values.
const rules = parseCss(template.replace(PLACEHOLDER, "matugen($1)"));

function expectScheme(rule: CssRule, scheme: "light" | "dark"): void {
  const wrong = COLOR_TOKENS.filter(
    (token) => rule.declarations.get(prop(token)) !== `matugen(colors.${token.replaceAll("-", "_")}.${scheme}.hex)`,
  );
  expect(wrong).toEqual([]);
  expect(rule.declarations.size).toBe(COLOR_TOKENS.length);
}

describe("contrib/matugen", () => {
  // spec: UI-091
  test("the template maps every color token for light and dark, with the built-in selectors, unlayered", () => {
    expect(rules.filter((rule) => rule.at.some((at) => at.startsWith("@layer")))).toEqual([]);
    expectScheme(findRule(rules, [], ":root"), "light");
    expectScheme(
      findRule(rules, ["@media (prefers-color-scheme: dark)"], ':root:not([data-color-scheme="light"])'),
      "dark",
    );
    expectScheme(findRule(rules, [], ':root[data-color-scheme="dark"]'), "dark");
    expect(rules).toHaveLength(3);
  });

  // spec: UI-091
  test("the README maps every token and the config defines the custom status colors", () => {
    const readme = readFileSync(join(MATUGEN_DIR, "README.md"), "utf8");
    const unmapped = COLOR_TOKENS.filter(
      (token) => !readme.includes(`| \`${prop(token)}\` | \`${token.replaceAll("-", "_")}\` |`),
    );
    expect(unmapped).toEqual([]);
    const config = readFileSync(join(MATUGEN_DIR, "config.toml"), "utf8");
    expect(config).toContain("[config.custom_colors]");
    expect(config).toMatch(/^success = /m);
    expect(config).toMatch(/^warning = /m);
    expect(config).toContain('output_path = "~/.config/omfp/theme.css"');
  });
});
