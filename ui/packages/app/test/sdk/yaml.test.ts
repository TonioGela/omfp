/** L0: `parseYaml`, the SDK's YAML parser (UI-571). */

import { describe, expect, test } from "bun:test";
import { parseYaml } from "../../src/sdk/index.ts";

describe("parseYaml", () => {
  // spec: UI-571
  test("gives each document's value, keys in order", async () => {
    expect(await parseYaml("name: omfp\nlist:\n  - 1\n  - two\nanchor: &a {x: 1}\nalias: *a\n")).toEqual({
      ok: true,
      documents: [{ name: "omfp", list: [1, "two"], anchor: { x: 1 }, alias: { x: 1 } }],
    });
    expect(await parseYaml("a: 1\n---\nb: 2\n")).toEqual({ ok: true, documents: [{ a: 1 }, { b: 2 }] });
    expect(await parseYaml("")).toEqual({ ok: true, documents: [] });
  });

  // spec: UI-571, UI-569
  test("an invalid text gives the first error with its line and column", async () => {
    const result = await parseYaml("a: 1\nb: [1, 2\nc: 3\n");
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect([result.line, result.column]).toEqual([3, 1]);
      expect(result.message).not.toContain("\n");
      expect(result.message).not.toContain(" at line ");
    }
    const bomb = await parseYaml(
      `a: &a ["x","x","x","x","x","x","x","x","x"]\n${Array.from({ length: 9 }, (_, index) => {
        const name = String.fromCharCode(98 + index);
        const previous = index === 0 ? "a" : String.fromCharCode(97 + index);
        return `${name}: &${name} [*${previous},*${previous},*${previous},*${previous},*${previous},*${previous},*${previous},*${previous},*${previous}]`;
      }).join("\n")}\n`,
    );
    expect(bomb.ok).toBe(false);
  });
});
