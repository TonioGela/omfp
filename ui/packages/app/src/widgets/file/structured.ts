/**
 * Structured text for `omfp:file`'s tree viewer (UI-567..570): a JSON parser (comments and trailing commas allowed for
 * JSONC) that keeps keys in order and numbers as written and reports errors with their line and column, YAML's
 * values as tree values, and the jq-style path of a node.
 */

/** A value of the tree: numbers keep their text (`12345678901234567890`, `1e3`). */
export type TreeValue =
  | { readonly kind: "object"; readonly entries: readonly (readonly [string, TreeValue])[] }
  | { readonly kind: "array"; readonly items: readonly TreeValue[] }
  | { readonly kind: "string"; readonly value: string }
  | { readonly kind: "number"; readonly text: string }
  | { readonly kind: "boolean"; readonly value: boolean }
  | { readonly kind: "null" };

/** Where and why a text does not parse (line and column from 1). */
export interface ParseError {
  readonly message: string;
  readonly line: number;
  readonly column: number;
}

export type ParseResult =
  | { readonly ok: true; readonly value: TreeValue }
  | { readonly ok: false; readonly error: ParseError };

/** Containers nested deeper than this are refused (the parser recurses). */
const MAX_DEPTH = 1000;
const NUMBER = /-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?/y;
const ESCAPES: Readonly<Record<string, string>> = {
  '"': '"',
  "\\": "\\",
  "/": "/",
  b: "\b",
  f: "\f",
  n: "\n",
  r: "\r",
  t: "\t",
};
/** JSON files that take comments by convention: TypeScript's and VS Code's. */
const JSONC_NAMES = /(?:^|\/)(?:(?:ts|js)config(?:\.[^/]+)?\.json|\.vscode\/[^/]+\.json)$/;

/** Whether the file at `path` is JSONC: a `.jsonc` file, `tsconfig*.json`, `jsconfig*.json` or JSON under `.vscode/`. */
export function isJsonc(path: string): boolean {
  return path.toLowerCase().endsWith(".jsonc") || JSONC_NAMES.test(path);
}

/** A failure at an offset of the text, turned into a line and column by {@link parseJson}. */
class Failure {
  constructor(
    readonly message: string,
    readonly offset: number,
  ) {}
}

/** The JSON value of `text`; with `comments` (JSONC), `//` and `/* *\/` comments and trailing commas are allowed. */
export function parseJson(text: string, comments: boolean): ParseResult {
  const parser = new Parser(text, comments);
  try {
    return { ok: true, value: parser.document() };
  } catch (thrown) {
    if (!(thrown instanceof Failure)) {
      throw thrown;
    }
    const before = text.slice(0, thrown.offset);
    const line = before.split("\n").length;
    return { ok: false, error: { message: thrown.message, line, column: thrown.offset - before.lastIndexOf("\n") } };
  }
}

class Parser {
  private at = 0;

  constructor(
    private readonly text: string,
    private readonly comments: boolean,
  ) {}

  document(): TreeValue {
    const value = this.value(0);
    this.space();
    if (this.at < this.text.length) {
      throw new Failure(`Unexpected \`${this.text[this.at]}\` after the value`, this.at);
    }
    return value;
  }

  /** Skips white space and, in JSONC, comments. */
  private space(): void {
    for (;;) {
      const rest = /[ \t\r\n]*/y;
      rest.lastIndex = this.at;
      rest.test(this.text);
      this.at = rest.lastIndex;
      if (!this.comments || this.text[this.at] !== "/") {
        return;
      }
      const next = this.text[this.at + 1];
      if (next === "/") {
        const end = this.text.indexOf("\n", this.at);
        this.at = end < 0 ? this.text.length : end;
      } else if (next === "*") {
        const end = this.text.indexOf("*/", this.at + 2);
        if (end < 0) {
          throw new Failure("Unterminated comment", this.at);
        }
        this.at = end + 2;
      } else {
        return;
      }
    }
  }

  private value(depth: number): TreeValue {
    this.space();
    const start = this.at;
    const char = this.text[start];
    if (char === undefined) {
      throw new Failure("Unexpected end of text", start);
    }
    if (char === "{" || char === "[") {
      if (depth >= MAX_DEPTH) {
        throw new Failure("Nested too deeply", start);
      }
      this.at += 1;
      return char === "{" ? this.object(depth + 1) : this.array(depth + 1);
    }
    if (char === '"') {
      return { kind: "string", value: this.string() };
    }
    NUMBER.lastIndex = start;
    if (NUMBER.test(this.text)) {
      this.at = NUMBER.lastIndex;
      return { kind: "number", text: this.text.slice(start, this.at) };
    }
    for (const [word, value] of [
      ["true", { kind: "boolean", value: true }],
      ["false", { kind: "boolean", value: false }],
      ["null", { kind: "null" }],
    ] as const) {
      if (this.text.startsWith(word, start)) {
        this.at += word.length;
        return value;
      }
    }
    throw new Failure(`Unexpected \`${char}\``, start);
  }

  /** The items up to `close`, each read by `item`, separated by commas (a trailing one allowed in JSONC). */
  private list(close: string, item: () => void): void {
    this.space();
    if (this.text[this.at] === close) {
      this.at += 1;
      return;
    }
    for (;;) {
      item();
      this.space();
      const char = this.text[this.at];
      this.at += 1;
      if (char === close) {
        return;
      }
      if (char !== ",") {
        throw new Failure(`Expected \`,\` or \`${close}\``, this.at - 1);
      }
      this.space();
      if (this.comments && this.text[this.at] === close) {
        this.at += 1;
        return;
      }
    }
  }

  private object(depth: number): TreeValue {
    const entries: [string, TreeValue][] = [];
    this.list("}", () => {
      this.space();
      if (this.text[this.at] !== '"') {
        throw new Failure("Expected a string key", this.at);
      }
      const key = this.string();
      this.space();
      if (this.text[this.at] !== ":") {
        throw new Failure("Expected `:`", this.at);
      }
      this.at += 1;
      entries.push([key, this.value(depth)]);
    });
    return { kind: "object", entries };
  }

  private array(depth: number): TreeValue {
    const items: TreeValue[] = [];
    this.list("]", () => {
      items.push(this.value(depth));
    });
    return { kind: "array", items };
  }

  /** The string starting at the current `"`, its escapes decoded. */
  private string(): string {
    const start = this.at;
    let value = "";
    let run = start + 1;
    for (let at = run; at < this.text.length; at += 1) {
      const code = this.text.charCodeAt(at);
      if (code === 34) {
        this.at = at + 1;
        return value + this.text.slice(run, at);
      }
      if (code < 0x20) {
        throw new Failure("Control character in a string", at);
      }
      if (code === 92) {
        value += this.text.slice(run, at);
        const escaped = this.text[at + 1] ?? "";
        const hex = /^[\da-fA-F]{4}$/.exec(this.text.slice(at + 2, at + 6));
        if (escaped === "u" && hex !== null) {
          value += String.fromCharCode(Number.parseInt(hex[0], 16));
          at += 5;
        } else if (Object.hasOwn(ESCAPES, escaped)) {
          value += ESCAPES[escaped] ?? "";
          at += 1;
        } else {
          throw new Failure("Invalid escape", at);
        }
        run = at + 1;
      }
    }
    throw new Failure("Unterminated string", start);
  }
}

/**
 * A JavaScript value (YAML's documents) as a tree value; what JSON has no word for (`undefined`) is `null`. Throws a
 * `RangeError` for values nested deeper than JSON's limit, which a recursive YAML alias makes endless.
 */
export function fromValue(value: unknown, depth = 0): TreeValue {
  if (depth > MAX_DEPTH) {
    throw new RangeError("Nested too deeply");
  }
  if (Array.isArray(value)) {
    return { kind: "array", items: value.map((item: unknown) => fromValue(item, depth + 1)) };
  }
  switch (typeof value) {
    case "string":
      return { kind: "string", value };
    case "number":
    case "bigint":
      return { kind: "number", text: String(value) };
    case "boolean":
      return { kind: "boolean", value };
    case "object":
      return value === null
        ? { kind: "null" }
        : {
            kind: "object",
            entries: Object.entries(value).map(([key, item]) => [key, fromValue(item, depth + 1)] as const),
          };
    default:
      return { kind: "null" };
  }
}

/** A key that reads bare in a path (`.name`); others are quoted (`["odd key"]`). */
const BARE_KEY = /^[A-Za-z_][\w]*$/;

/** The jq-style path of a node from the root: `.`, `.a[0].b`, `.["odd key"]`. */
export function pathText(path: readonly (string | number)[]): string {
  if (path.length === 0) {
    return ".";
  }
  return path
    .map((segment) =>
      typeof segment === "number"
        ? `[${segment}]`
        : BARE_KEY.test(segment)
          ? `.${segment}`
          : `[${JSON.stringify(segment)}]`,
    )
    .join("")
    .replace(/^\[/, ".[");
}

/** A value as text to copy: a container as JSON two spaces deep (numbers as written), a scalar as its text. */
export function valueText(value: TreeValue, indent = ""): string {
  const inner = `${indent}  `;
  switch (value.kind) {
    case "object":
      return value.entries.length === 0
        ? "{}"
        : `{\n${value.entries.map(([key, item]) => `${inner}${JSON.stringify(key)}: ${jsonText(item, inner)}`).join(",\n")}\n${indent}}`;
    case "array":
      return value.items.length === 0
        ? "[]"
        : `[\n${value.items.map((item) => `${inner}${jsonText(item, inner)}`).join(",\n")}\n${indent}]`;
    case "string":
      return value.value;
    case "number":
      return value.text;
    case "boolean":
      return String(value.value);
    case "null":
      return "null";
  }
}

/** A value inside a container: as {@link valueText}, strings quoted. */
function jsonText(value: TreeValue, indent: string): string {
  return value.kind === "string" ? JSON.stringify(value.value) : valueText(value, indent);
}
