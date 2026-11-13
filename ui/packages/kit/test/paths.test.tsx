/** L0/L4: file paths in text (UI-116): what reads as a path, and Markdown that opens them. */

import { afterEach, describe, expect, mock, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  linkReference,
  Markdown,
  type MarkdownImageProps,
  type PathReference,
  pathReference,
  relativeSource,
  resolvePath,
} from "../src/index.ts";

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

  // spec: UI-561
  test("a relative target resolves against a folder: `.` and `..` segments fold, `..` past the root stays", () => {
    expect(resolvePath("docs/specs", "04-ui.md")).toBe("docs/specs/04-ui.md");
    expect(resolvePath("docs/specs", "./a/../b.md")).toBe("docs/specs/b.md");
    expect(resolvePath("docs/specs", "../../README.md")).toBe("README.md");
    expect(resolvePath("", "a.md")).toBe("a.md");
    expect(resolvePath("docs", "../../x.md")).toBe("../x.md");
    expect(resolvePath("docs/", "a//b.md")).toBe("docs/a/b.md");
  });

  // spec: UI-562
  test("an image source is a file path when it is relative: not a URL, a data URL, an absolute path or empty", () => {
    expect(relativeSource("img/a.png")).toBe("img/a.png");
    expect(relativeSource("../a%20b.png")).toBe("../a b.png");
    expect(relativeSource("a.png?raw=1#x")).toBe("a.png");
    for (const src of [
      "https://example.com/a.png",
      "data:image/png;base64,AA",
      "/abs.png",
      "//cdn/a.png",
      "",
      undefined,
    ]) {
      expect([src, relativeSource(src)]).toEqual([src, undefined]);
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

  // spec: UI-561
  test("with linkBase, relative links open the file of that folder; inline code paths stay as written", async () => {
    const opened: PathReference[] = [];
    render(
      <Markdown
        onOpenPath={(reference) => opened.push(reference)}
        linkBase="docs/guide"
        text={"[next](next.md#L4), [up](../../README.md) and `src/lib.rs`."}
      />,
    );
    const links = screen.getAllByTestId("path-link");
    expect(links.map((link) => link.dataset.omfpPath)).toEqual(["docs/guide/next.md", "README.md", "src/lib.rs"]);
    const user = userEvent.setup();
    for (const link of links) {
      await user.click(link);
    }
    expect(opened).toEqual([
      { path: "docs/guide/next.md", line: 4 },
      { path: "README.md", line: undefined },
      { path: "src/lib.rs", line: undefined },
    ]);
  });
});

/** Names a relative image's resolved path, alt text and title. */
function Image({ path, alt, title }: MarkdownImageProps) {
  return <span data-testid="image" data-path={path} data-alt={alt} data-title={title} />;
}

describe("Markdown with an image component", () => {
  // spec: UI-562, UI-040
  test("relative images render through it with their resolved path; remote images stay plain images", () => {
    render(
      <Markdown
        data-testid="md"
        image={Image}
        linkBase="docs"
        text={'![chart](img/chart.png "The chart") ![logo](https://example.com/logo.png)'}
      />,
    );
    const image = screen.getByTestId("image");
    expect([image.dataset.path, image.dataset.alt, image.dataset.title]).toEqual([
      "docs/img/chart.png",
      "chart",
      "The chart",
    ]);
    const plain = screen.getByTestId("md").querySelectorAll("img");
    expect([...plain].map((img) => img.getAttribute("src"))).toEqual(["https://example.com/logo.png"]);
  });
});
