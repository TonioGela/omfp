/**
 * A tiny CSS reader for the style tests: enough for our own files (at-rules, flat style rules, no nesting, no braces
 * inside strings), not a general parser.
 */
import { readdirSync } from "node:fs";
import { join } from "node:path";

export interface CssRule {
  /** Enclosing at-rule preludes, outermost first (e.g. `@layer omfp-tokens`, `@media (…)`). */
  readonly at: readonly string[];
  /** Selector list as written, whitespace collapsed. */
  readonly selector: string;
  readonly declarations: ReadonlyMap<string, string>;
}

const COMMENT = /\/\*[\s\S]*?\*\//g;

export function stripComments(css: string): string {
  return css.replace(COMMENT, "");
}

function collapse(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

function parseDeclarations(body: string): Map<string, string> {
  const declarations = new Map<string, string>();
  for (const part of body.split(";")) {
    const colon = part.indexOf(":");
    if (colon > 0) {
      declarations.set(part.slice(0, colon).trim(), collapse(part.slice(colon + 1)));
    }
  }
  return declarations;
}

function closingBrace(css: string, open: number): number {
  let depth = 0;
  for (let index = open; index < css.length; index += 1) {
    if (css[index] === "{") depth += 1;
    if (css[index] === "}") {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  throw new Error(`unbalanced braces at ${open}`);
}

function parseBlock(css: string, at: readonly string[], rules: CssRule[]): void {
  let cursor = 0;
  while (cursor < css.length) {
    const open = css.indexOf("{", cursor);
    const semicolon = css.indexOf(";", cursor);
    if (open < 0) return;
    if (semicolon >= 0 && semicolon < open) {
      // A statement at-rule such as `@layer a, b;`.
      cursor = semicolon + 1;
      continue;
    }
    const prelude = collapse(css.slice(cursor, open));
    const close = closingBrace(css, open);
    const body = css.slice(open + 1, close);
    if (prelude.startsWith("@")) {
      parseBlock(body, [...at, prelude], rules);
    } else {
      rules.push({ at, selector: prelude, declarations: parseDeclarations(body) });
    }
    cursor = close + 1;
  }
}

export function parseCss(css: string): CssRule[] {
  const rules: CssRule[] = [];
  parseBlock(stripComments(css), [], rules);
  return rules;
}

export const KIT_SRC = join(import.meta.dir, "..", "..", "src");
export const MATUGEN_DIR = join(import.meta.dir, "..", "..", "..", "..", "..", "contrib", "matugen");

/** Every `.css` file under `dir`, recursively, as absolute paths. */
export function cssFiles(dir: string): string[] {
  return readdirSync(dir, { recursive: true, encoding: "utf8" })
    .filter((name) => name.endsWith(".css"))
    .map((name) => join(dir, name))
    .sort();
}

/** The rule (exactly one) whose enclosing at-rules and selector match. */
export function findRule(rules: readonly CssRule[], at: readonly string[], selector: string): CssRule {
  const found = rules.filter(
    (rule) => rule.selector === selector && rule.at.length === at.length && rule.at.every((x, i) => x === at[i]),
  );
  if (found.length !== 1) {
    throw new Error(`expected one rule ${[...at, selector].join(" > ")}, found ${found.length}`);
  }
  const [rule] = found;
  if (rule === undefined) throw new Error("unreachable");
  return rule;
}
