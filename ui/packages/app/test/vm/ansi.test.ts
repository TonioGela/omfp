/** L0: ANSI escape sequences of command output turned into styled runs (the bash card, UI-050). */

import { describe, expect, test } from "bun:test";
import { ansiRuns } from "../../src/vm/ansi.ts";

describe("ansi", () => {
  // spec: UI-050
  test("plain text is one run without style", () => {
    expect(ansiRuns("hello\nworld")).toEqual([{ text: "hello\nworld", style: [] }]);
    expect(ansiRuns("")).toEqual([]);
  });

  // spec: UI-050
  test("SGR codes set colors and attributes until reset", () => {
    expect(ansiRuns("a\u001b[1;31mred\u001b[0m b \u001b[32;4mgreen\u001b[39m under\u001b[m")).toEqual([
      { text: "a", style: [] },
      { text: "red", style: ["fg-red", "bold"] },
      { text: " b ", style: [] },
      { text: "green", style: ["fg-green", "underline"] },
      { text: " under", style: ["underline"] },
    ]);
    expect(ansiRuns("\u001b[91;44mx\u001b[22;49my")).toEqual([
      { text: "x", style: ["fg-bright-red", "bg-blue"] },
      { text: "y", style: ["fg-bright-red"] },
    ]);
  });

  // spec: UI-050
  test("256-color and true-color codes keep the text; other escape sequences are dropped", () => {
    expect(ansiRuns("\u001b[38;5;208ma\u001b[38;2;1;2;3mb\u001b[0m")).toEqual([{ text: "ab", style: [] }]);
    expect(ansiRuns("\u001b[2K\u001b[1Gline\u001b]0;title\u0007 end")).toEqual([{ text: "line end", style: [] }]);
  });
});
