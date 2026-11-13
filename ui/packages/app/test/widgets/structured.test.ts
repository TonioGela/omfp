/** L0: the JSON / JSONC parser, values and paths of `omfp:file`'s tree viewer (UI-567..570). */

import { describe, expect, test } from "bun:test";
import {
  fromValue,
  isJsonc,
  parseJson,
  pathText,
  type TreeValue,
  valueText,
} from "../../src/widgets/file/structured.ts";

/** A parsed text's value, or its error as `message @ line:column`. */
function parsed(text: string, comments = false): TreeValue | string {
  const result = parseJson(text, comments);
  return result.ok ? result.value : `${result.error.message} @ ${result.error.line}:${result.error.column}`;
}

describe("parseJson", () => {
  // spec: UI-570
  test("objects keep their keys in order (duplicates too), numbers their text, strings their escapes decoded", () => {
    expect(parsed('{"b": [1, -2.5e3, true, null], "a": "x\\n\\u00e9\\"", "b": {}}')).toEqual({
      kind: "object",
      entries: [
        [
          "b",
          {
            kind: "array",
            items: [
              { kind: "number", text: "1" },
              { kind: "number", text: "-2.5e3" },
              { kind: "boolean", value: true },
              { kind: "null" },
            ],
          },
        ],
        ["a", { kind: "string", value: 'x\né"' }],
        ["b", { kind: "object", entries: [] }],
      ],
    });
    expect(parsed(" 12345678901234567890 ")).toEqual({ kind: "number", text: "12345678901234567890" });
  });

  // spec: UI-569, UI-570
  test("invalid JSON names the problem with its line and column", () => {
    expect(parsed('{\n  "a": 1,\n}')).toBe("Expected a string key @ 3:1");
    expect(parsed('{"a" 1}')).toBe("Expected `:` @ 1:6");
    expect(parsed("[1 2]")).toBe("Expected `,` or `]` @ 1:4");
    expect(parsed('{"a": "open')).toBe("Unterminated string @ 1:7");
    expect(parsed("[01]")).toBe("Expected `,` or `]` @ 1:3");
    expect(parsed("[1] x")).toBe("Unexpected `x` after the value @ 1:5");
    expect(parsed("")).toBe("Unexpected end of text @ 1:1");
    expect(parsed('["a\tb"]')).toBe("Control character in a string @ 1:4");
    expect(parsed('["\\x"]')).toBe("Invalid escape @ 1:3");
    expect(parsed("// note\n{}")).toBe("Unexpected `/` @ 1:1");
    expect(parsed(`${"[".repeat(2000)}${"]".repeat(2000)}`)).toBe("Nested too deeply @ 1:1001");
    expect(parsed('{"a": yes}')).toBe("Unexpected `y` @ 1:7");
  });

  // spec: UI-570
  test("JSONC allows line and block comments and trailing commas, and still reports errors", () => {
    expect(parsed('// settings\n{\n  "a": 1, /* one */\n  "b": [2,],\n}\n', true)).toEqual({
      kind: "object",
      entries: [
        ["a", { kind: "number", text: "1" }],
        ["b", { kind: "array", items: [{ kind: "number", text: "2" }] }],
      ],
    });
    expect(parsed('{"a": "// not a comment"}', true)).toEqual({
      kind: "object",
      entries: [["a", { kind: "string", value: "// not a comment" }]],
    });
    expect(parsed("{\n  /* open", true)).toBe("Unterminated comment @ 2:3");
  });

  // spec: UI-570
  test("JSONC is .jsonc and the JSON files known to take comments", () => {
    const paths = ["a.jsonc", "tsconfig.json", "web/tsconfig.app.json", "jsconfig.json", ".vscode/settings.json"];
    expect(paths.map(isJsonc)).toEqual([true, true, true, true, true]);
    expect(["package.json", "data/tsconfig.txt", "vscode/settings.json"].map(isJsonc)).toEqual([false, false, false]);
  });
});

describe("tree values", () => {
  // spec: UI-567
  test("YAML's JavaScript values become tree values", () => {
    expect(fromValue({ a: [1, "x", false, null], b: { c: 2.5 } })).toEqual({
      kind: "object",
      entries: [
        [
          "a",
          {
            kind: "array",
            items: [
              { kind: "number", text: "1" },
              { kind: "string", value: "x" },
              { kind: "boolean", value: false },
              { kind: "null" },
            ],
          },
        ],
        ["b", { kind: "object", entries: [["c", { kind: "number", text: "2.5" }]] }],
      ],
    });
    expect(fromValue(undefined)).toEqual({ kind: "null" });
  });

  // spec: UI-568
  test("a node's path reads as jq's: `.key`, `[\"odd key\"]`, `[index]`", () => {
    expect(pathText([])).toBe(".");
    expect(pathText(["a", 0, "b_c"])).toBe(".a[0].b_c");
    expect(pathText(["odd key", "1x", 2])).toBe('.["odd key"]["1x"][2]');
  });
});

describe("value text", () => {
  // spec: UI-568
  test("a value copies as JSON, two spaces deep, numbers as written; a scalar as its text", () => {
    const value = parseJson('{"a": [1.50, "x\\ny"], "b": {}, "c": [], "d": null}', false);
    expect(value.ok && valueText(value.value)).toBe(
      '{\n  "a": [\n    1.50,\n    "x\\ny"\n  ],\n  "b": {},\n  "c": [],\n  "d": null\n}',
    );
    expect([
      valueText({ kind: "string", value: "plain" }),
      valueText({ kind: "number", text: "1e3" }),
      valueText({ kind: "boolean", value: false }),
      valueText({ kind: "null" }),
    ]).toEqual(["plain", "1e3", "false", "null"]);
  });
});
