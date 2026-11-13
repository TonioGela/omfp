/** L4: `omfp:file`'s viewers of text formats over a fake client: Markdown preview (UI-560..562), table (UI-563..565). */

import { afterAll, afterEach, beforeAll, describe, expect, test } from "bun:test";
import type { FileContent } from "@omfp/protocol";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Markdown } from "omfp/kit";
import { type FilePoll, OPEN_FILES, SELECTED_THREAD } from "../../src/sdk/index.ts";
import { FileWidget } from "../../src/widgets/file/index.ts";
import { FileImage, FileUrlHook } from "../../src/widgets/file/markdown.tsx";
import { viewerOf } from "../../src/widgets/file/viewers.tsx";
import { repo, thread, workspace } from "../support/fixtures.ts";
import { FakeClient, renderWithHost, testHost } from "../support/host.tsx";

afterEach(cleanup);

/** A poll that never runs: these tests read each file once. */
const NO_POLL: FilePoll = () => () => undefined;

function text(path: string, content: string, extra: Partial<FileContent> = {}): FileContent {
  return {
    path,
    kind: "text",
    size: content.length,
    hash: `h-${content.length}`,
    unchanged: false,
    content,
    truncated: false,
    repo: "fe",
    head: content,
    ...extra,
  };
}

/** `omfp:file` showing `path`, which holds `content` on disk (in the repository `fe`). */
function setup(path: string, content: string, extra: Partial<FileContent> = {}) {
  const base = testHost(new FakeClient(), { filePoll: NO_POLL });
  base.host.window.set(SELECTED_THREAD, "t1");
  base.client.answers.set("workspaces.read_file", () => text(path, content, extra));
  const test = renderWithHost(<FileWidget path={path} />, base);
  test.client.connected();
  test.client.index({
    workspaces: [workspace("w1", [repo("w1", "fe", "feat")])],
    threads: [thread("t1", "one", "w1")],
  });
  test.flush();
  return { ...test, user: userEvent.setup() };
}

async function editorText(): Promise<string> {
  const editor = await screen.findByTestId("file-editor");
  let shown = "";
  await waitFor(() => {
    expect(editor.querySelector(".cm-content")).not.toBeNull();
    shown = [...editor.querySelectorAll(".cm-line")].map((line) => line.textContent).join("\n");
  });
  return shown;
}

const README =
  "# Guide\n\nRead [the next part](next.md#L3) and [the root](../README.md).\n\n<script>window.pwned = 1</script>\n";

describe("Markdown preview", () => {
  // spec: UI-560
  test("Markdown files read-only show the preview; the toggle shows the source and back; other files have none", async () => {
    const test = setup("fe/docs/guide.md", README);
    const preview = await screen.findByTestId("file-viewer");
    expect(preview.dataset.viewer).toBe("markdown");
    expect(preview.querySelector("h1")?.textContent).toBe("Guide");
    // No raw HTML (UI-040).
    expect(preview.querySelector("script")).toBeNull();
    expect(screen.queryAllByTestId("file-editor").length).toBe(0);
    const toggle = screen.getByTestId("file-view-toggle");
    expect([toggle.textContent, toggle.getAttribute("aria-pressed")]).toEqual(["Preview", "true"]);
    await test.user.click(toggle);
    expect(await editorText()).toBe(README);
    expect(screen.queryAllByTestId("file-viewer").length).toBe(0);
    expect(screen.getByTestId("file-view-toggle").getAttribute("aria-pressed")).toBe("false");
    await test.user.click(screen.getByTestId("file-view-toggle"));
    expect((await screen.findByTestId("file-viewer")).dataset.viewer).toBe("markdown");
    cleanup();
    for (const path of ["notes.markdown", "page.mdx", "README.MD"]) {
      setup(path, "# Title\n");
      expect([path, (await screen.findByTestId("file-viewer")).dataset.viewer]).toEqual([path, "markdown"]);
      cleanup();
    }
    setup("src/lib.rs", "fn one() {}\n");
    expect(await editorText()).toBe("fn one() {}\n");
    expect(screen.queryAllByTestId("file-view-toggle").length).toBe(0);
  });

  // spec: UI-560
  test("editing shows the source and Done the preview again; the diff takes the preview's place while it is on", async () => {
    const test = setup("fe/docs/guide.md", README);
    await screen.findByTestId("file-viewer");
    await test.user.click(screen.getByTestId("file-edit"));
    expect(await editorText()).toBe(README);
    expect(screen.getByTestId("file-editor").dataset.readOnly).toBe("false");
    expect(screen.queryAllByTestId("file-view-toggle").length).toBe(0);
    await test.user.click(screen.getByTestId("file-done"));
    expect((await screen.findByTestId("file-viewer")).dataset.viewer).toBe("markdown");
    await test.user.click(screen.getByTestId("file-diff"));
    await screen.findByTestId("file-head-diff");
    expect(screen.queryAllByTestId("file-viewer").length).toBe(0);
    expect(screen.getByTestId("file-view-toggle").getAttribute("aria-pressed")).toBe("false");
    await test.user.click(screen.getByTestId("file-view-toggle"));
    expect(screen.queryAllByTestId("file-head-diff").length).toBe(0);
    expect(screen.getByTestId("file-diff").getAttribute("aria-pressed")).toBe("false");
    await screen.findByTestId("file-viewer");
  });

  // spec: UI-561
  test("relative links open the file they name relative to the Markdown file's folder", async () => {
    const test = setup("fe/docs/guide.md", README);
    await screen.findByTestId("file-viewer");
    const links = screen.getAllByTestId("path-link");
    expect(links.map((link) => link.dataset.omfpPath)).toEqual(["fe/docs/next.md", "fe/README.md"]);
    await test.user.click(links[0] ?? document.body);
    expect(test.host.window.get(OPEN_FILES).w1?.tabs.map(({ path, line }) => ({ path, line }))).toEqual([
      { path: "fe/docs/next.md", line: 3 },
    ]);
  });

  // spec: UI-562
  test("relative images show their alt text until the SDK serves files by URL", async () => {
    setup("fe/docs/guide.md", "![the chart](img/chart.png)\n\n![remote](https://example.com/r.png)\n");
    const preview = await screen.findByTestId("file-viewer");
    expect(screen.getByTestId("file-image-alt").textContent).toBe("the chart");
    expect([...preview.querySelectorAll("img")].map((img) => img.getAttribute("src"))).toEqual([
      "https://example.com/r.png",
    ]);
  });

  // spec: UI-562
  test("with a file URL hook, relative images load from it; without a URL yet they show their alt text", () => {
    const urls: Record<string, string> = { "docs/img/chart.png": "/files/w1/docs/img/chart.png" };
    render(
      <FileUrlHook.Provider value={(path) => urls[path]}>
        <Markdown
          data-testid="md"
          linkBase="docs"
          image={FileImage}
          text={"![chart](img/chart.png) ![gone](gone.png)"}
        />
      </FileUrlHook.Provider>,
    );
    const image = screen.getByTestId("md").querySelector("img");
    expect([image?.getAttribute("src"), image?.getAttribute("alt")]).toEqual(["/files/w1/docs/img/chart.png", "chart"]);
    expect(screen.getByTestId("file-image-alt").textContent).toBe("gone");
  });

  // spec: UI-560, UI-563
  test("the viewer is chosen by the file's extension, case aside", () => {
    const paths = ["a.md", "b/c.MARKDOWN", "d.mdx", "e.rs", "md", "f.md.bak", "g.csv", "h.Tsv", "csv"];
    expect(paths.map((path) => viewerOf(path)?.name)).toEqual([
      "markdown",
      "markdown",
      "markdown",
      undefined,
      undefined,
      undefined,
      "table",
      "table",
      undefined,
    ]);
  });
});

/** The cells of the table viewer's rendered body rows. */
function bodyRows(): string[][] {
  return [...screen.getByTestId("file-table").querySelectorAll("tbody tr[data-index]")].map((row) =>
    [...row.querySelectorAll("td")].map((cell) => cell.textContent ?? ""),
  );
}

const CSV = 'name,size,note\nbeta,10,"two, words"\nalpha,9,\ngamma,100,"line\nbreak"\n';

describe("table viewer", () => {
  // happy-dom lays nothing out: the table's scroll box gets a 600 px view, the height the virtualizer reads.
  const clientHeight = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientHeight");
  beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, "clientHeight", {
      configurable: true,
      get(this: HTMLElement) {
        return this.classList.contains("omfp-file__table") ? 600 : 0;
      },
    });
  });
  afterAll(() => {
    if (clientHeight === undefined) {
      Reflect.deleteProperty(HTMLElement.prototype, "clientHeight");
    } else {
      Object.defineProperty(HTMLElement.prototype, "clientHeight", clientHeight);
    }
  });

  // spec: UI-563, UI-566
  test("CSV and TSV files show as a table under their first record; the toggle shows the source", async () => {
    const test = setup("fe/data/sizes.csv", CSV);
    const viewer = await screen.findByTestId("file-viewer");
    expect(viewer.dataset.viewer).toBe("table");
    expect([...viewer.querySelectorAll("thead th")].map((cell) => cell.textContent)).toEqual(["name", "size", "note"]);
    expect(bodyRows()).toEqual([
      ["beta", "10", "two, words"],
      ["alpha", "9", ""],
      ["gamma", "100", "line\nbreak"],
    ]);
    expect(screen.getByTestId("file-table-summary").textContent).toBe("3 rows, 3 columns");
    const toggle = screen.getByTestId("file-view-toggle");
    expect([toggle.textContent, toggle.getAttribute("aria-pressed")]).toEqual(["Table", "true"]);
    await test.user.click(toggle);
    expect((await editorText()).split("\n")[0]).toBe("name,size,note");
    cleanup();
    setup("fe/data/sizes.TSV", "a\tb\n1\t2\n");
    await screen.findByTestId("file-viewer");
    expect(bodyRows()).toEqual([["1", "2"]]);
  });

  // spec: UI-564
  test("a click on a header sorts by its column, numbers by value, ascending then descending", async () => {
    const test = setup("fe/data/sizes.csv", CSV);
    await screen.findByTestId("file-viewer");
    const header = () => screen.getAllByTestId("file-table-sort")[1] ?? document.body;
    const sorted = () => screen.getByTestId("file-table").querySelectorAll("th")[1]?.getAttribute("aria-sort");
    expect(sorted()).toBe("none");
    await test.user.click(header());
    expect([sorted(), bodyRows().map((row) => row[0])]).toEqual(["ascending", ["alpha", "beta", "gamma"]]);
    await test.user.click(header());
    expect([sorted(), bodyRows().map((row) => row[0])]).toEqual(["descending", ["gamma", "beta", "alpha"]]);
    // Another column sorts ascending first; empty cells go last.
    await test.user.click(screen.getAllByTestId("file-table-sort")[2] ?? document.body);
    expect(bodyRows().map((row) => row[0])).toEqual(["gamma", "beta", "alpha"]);
    expect(sorted()).toBe("none");
  });

  // spec: UI-565
  test("only the rows in view are rendered; a file the daemon cut says so and drops its incomplete last row", async () => {
    const rows = Array.from({ length: 5000 }, (_, index) => `row ${index},${index}`);
    setup("big.csv", `name,n\n${rows.join("\n")}\n`);
    await screen.findByTestId("file-viewer");
    expect(screen.getByTestId("file-table-summary").textContent).toBe("5000 rows, 2 columns");
    const rendered = bodyRows().length;
    expect(rendered).toBeGreaterThan(0);
    expect(rendered).toBeLessThan(100);
    expect(screen.getByTestId("file-table").querySelector("table")?.getAttribute("aria-rowcount")).toBe("5001");
    // Scrolled to the 3000th row, the rows around it replace the first ones.
    const scroller = screen.getByTestId("file-table");
    Object.defineProperty(scroller, "scrollTop", { configurable: true, value: 3000 * 28 });
    act(() => {
      fireEvent.scroll(scroller);
    });
    await waitFor(() => expect(bodyRows().some((row) => row[0] === "row 3000")).toBe(true));
    expect(bodyRows().some((row) => row[0] === "row 0")).toBe(false);
    expect(bodyRows().length).toBeLessThan(100);
    cleanup();
    setup("cut.csv", 'name,n\na,1\nb,"2', { truncated: true, size: 4 * 1024 * 1024 });
    await screen.findByTestId("file-viewer");
    expect(bodyRows()).toEqual([["a", "1"]]);
    expect(screen.getByTestId("file-table-summary").textContent).toBe(
      "1 row, 2 columns: the first rows of this 4.0 MiB file, which is too large to read whole",
    );
  });
});
