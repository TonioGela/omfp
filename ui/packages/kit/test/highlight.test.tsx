import { afterEach, describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { grammarOf, highlightLines } from "../src/highlight.tsx";
import { Code, Diff, languageOfPath, loadLanguage, Markdown } from "../src/index.ts";
import { KIT_SRC, parseCss } from "./support/css.ts";

afterEach(cleanup);

describe("highlighting", () => {
  // spec: UI-090
  test("languages come from fence names, aliases and file names", () => {
    expect(["Rust", "rs", "tsx", "sh", "yml", "html", "brainfuck"].map(grammarOf)).toEqual([
      "rust",
      "rust",
      "typescript",
      "bash",
      "yaml",
      "xml",
      undefined,
    ]);
    expect(
      ["src/lib.rs", "ui/app.test.tsx", "Dockerfile", "notes.md", "README", ".gitignore", "a/b.unknown"].map(
        languageOfPath,
      ),
    ).toEqual(["rust", "typescript", "dockerfile", "markdown", undefined, undefined, undefined]);
  });

  // spec: UI-090
  test("a grammar loads on demand, then code splits into classed tokens per line", async () => {
    expect(highlightLines("fn main() {}", "python")).toBeUndefined();
    await loadLanguage("python");
    const lines = highlightLines('def f():\n    """doc\n    more"""\n', "py");
    expect(lines?.length).toBe(4);
    expect(lines?.[0]?.[0]).toEqual({ text: "def", className: "hljs-keyword" });
    // A token spanning lines keeps its class on each line.
    expect(lines?.[2]).toEqual([{ text: '    more"""', className: "hljs-string" }]);
    expect(await loadLanguage("brainfuck").catch((error: Error) => error.message)).toBe("no grammar for brainfuck");
  });

  // spec: UI-090
  test("a code block renders plain text first, then highlighted once its grammar arrives", async () => {
    render(<Code code={"fn main() {\n    let x = 1;\n}"} language="rust" block data-testid="code" />);
    const code = screen.getByTestId("code");
    expect(code.textContent).toBe("fn main() {\n    let x = 1;\n}");
    await waitFor(() => expect(code.querySelector(".hljs-keyword")?.textContent).toBe("fn"));
    expect(code.textContent).toBe("fn main() {\n    let x = 1;\n}");
    render(<Code code="plain words" language="no-such-language" block data-testid="plain" />);
    expect(screen.getByTestId("plain").querySelector("span")).toBeNull();
  });

  // spec: UI-090, UI-040
  test("fenced Markdown code with a language is highlighted; inline code is not", async () => {
    render(<Markdown text={"Use `let`:\n\n```rust\nlet x = 1;\n```"} data-testid="md" />);
    const root = screen.getByTestId("md");
    await waitFor(() => expect(root.querySelector("pre .hljs-keyword")?.textContent).toBe("let"));
    expect(root.querySelector("pre code")?.className).toContain("language-rust");
    expect(root.querySelector("p code")?.querySelector("span")).toBeNull();
  });

  // spec: UI-090, UI-080
  test("a diff highlights each side as a whole", async () => {
    const patch = "@@ -1,2 +1,2 @@\n /* a\n-   old */ fn a() {}\n+   new */ fn b() {}\n";
    render(<Diff patch={patch} language="rust" data-testid="diff" />);
    const diff = screen.getByTestId("diff");
    await waitFor(() => expect(diff.querySelector("[data-kind=add] .hljs-comment")?.textContent).toBe("   new */"));
    expect(diff.querySelector("[data-kind=del] .hljs-comment")?.textContent).toBe("   old */");
    expect(diff.querySelector("[data-kind=add] .hljs-keyword")?.textContent).toBe("fn");
  });

  // spec: UI-090, UI-091
  test("token colors come only from the theme's role tokens", () => {
    const rules = parseCss(readFileSync(join(KIT_SRC, "kit.css"), "utf8")).filter((rule) =>
      rule.selector.includes(".hljs-"),
    );
    expect(rules.length).toBeGreaterThan(5);
    const colors = rules.flatMap((rule) => [...rule.declarations].filter(([property]) => property === "color"));
    expect(colors.length).toBeGreaterThan(5);
    expect(colors.filter(([, value]) => !/^var\(--omfp-[a-z-]+\)$/.test(value))).toEqual([]);
  });
});
