/** L0/L4: file paths in text (UI-116): what reads as a path, and Markdown that opens them. */

import { afterEach, describe, expect, mock, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { linkReference, Markdown, type PathReference, pathReference } from "../src/index.ts";

afterEach(cleanup);

describe("pathReference", () => {
  // spec: UI-116
  test("paths with a slash or a known extension, with their line; not URLs, folders, prose or calls", () => {
    expect(pathReference("src/lib.rs")).toEqual({ path: "src/lib.rs", line: undefined });
    expect(pathReference("src/lib.rs:12")).toEqual({ path: "src/lib.rs", line: 12 });
    expect(pathReference("src/lib.rs:12:4")).toEqual({ path: "src/lib.rs", line: 12 });
    expect(pathReference("README.md")).toEqual({ path: "README.md", line: undefined });
    expect(pathReference("./ui/packages/app/build.ts")?.path).toBe("./ui/packages/app/build.ts");
    expect(pathReference("~/notes/todo.txt")?.path).toBe("~/notes/todo.txt");
    expect(pathReference("/home/u/ws/fe/src/main.ts")?.path).toBe("/home/u/ws/fe/src/main.ts");
    expect(pathReference("Makefile")?.path).toBe("Makefile");
    expect(pathReference("crates/omfp/Cargo.toml")?.path).toBe("crates/omfp/Cargo.toml");
    for (const text of [
      "https://example.com/a.rs",
      "src/",
      "let x = 1",
      "console.log",
      "useFile()",
      "foo",
      "e.g.",
      "",
      "C:/x.rs",
    ]) {
      expect([text, pathReference(text)]).toEqual([text, undefined]);
    }
  });

  // spec: UI-116
  test("a Markdown link's relative target is a path, with a `#L12` line; absolute and other links are not", () => {
    expect(linkReference("docs/specs/04-ui.md")).toEqual({ path: "docs/specs/04-ui.md", line: undefined });
    expect(linkReference("src/lib.rs#L12")).toEqual({ path: "src/lib.rs", line: 12 });
    expect(linkReference("my%20notes/a.md")).toEqual({ path: "my notes/a.md", line: undefined });
    for (const href of ["https://example.com/x.md", "#section", "/abs/x.md", "mailto:a@b.c", undefined, ""]) {
      expect(linkReference(href)).toBeUndefined();
    }
  });
});

describe("Markdown with onOpenPath", () => {
  // spec: UI-116
  test("inline code paths and relative links open the file; other code, fenced blocks and URLs stay as they are", async () => {
    const opened: PathReference[] = [];
    const onOpen = mock((reference: PathReference) => {
      opened.push(reference);
    });
    render(
      <Markdown
        data-testid="md"
        onOpenPath={onOpen}
        text={
          "See `src/lib.rs:3`, [the spec](docs/a.md#L7), `let x` and [the site](https://example.com).\n\n```\nsrc/lib.rs\n```"
        }
      />,
    );
    const links = screen.getAllByTestId("path-link");
    expect(links.map((link) => [link.textContent, link.dataset.omfpPath, link.dataset.omfpLine])).toEqual([
      ["src/lib.rs:3", "src/lib.rs", "3"],
      ["the spec", "docs/a.md", "7"],
    ]);
    const user = userEvent.setup();
    for (const link of links) {
      await user.click(link);
    }
    expect(opened).toEqual([
      { path: "src/lib.rs", line: 3 },
      { path: "docs/a.md", line: 7 },
    ]);
    const root = screen.getByTestId("md");
    expect(root.querySelector("a[href='https://example.com']")?.getAttribute("target")).toBe("_blank");
    expect([...root.querySelectorAll("code")].map((code) => code.textContent)).toEqual([
      "src/lib.rs:3",
      "let x",
      "src/lib.rs\n",
    ]);
  });

  // spec: UI-040, UI-116
  test("without onOpenPath paths stay text", () => {
    render(<Markdown data-testid="md" text="See `src/lib.rs` and [a](docs/a.md)." />);
    expect(screen.queryByTestId("path-link")).toBeNull();
    expect(screen.getByTestId("md").querySelector("a")?.getAttribute("href")).toBe("docs/a.md");
  });
});
