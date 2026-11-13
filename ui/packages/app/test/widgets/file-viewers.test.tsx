/** L4: `omfp:file`'s viewers of text formats over a fake client: Markdown preview (UI-560..562). */

import { afterEach, describe, expect, test } from "bun:test";
import type { FileContent } from "@omfp/protocol";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
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

function text(path: string, content: string): FileContent {
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
  };
}

/** `omfp:file` showing `path`, which holds `content` on disk (in the repository `fe`). */
function setup(path: string, content: string) {
  const base = testHost(new FakeClient(), { filePoll: NO_POLL });
  base.host.window.set(SELECTED_THREAD, "t1");
  base.client.answers.set("workspaces.read_file", () => text(path, content));
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

  // spec: UI-560
  test("the viewer is chosen by the file's extension, case aside", () => {
    expect(["a.md", "b/c.MARKDOWN", "d.mdx", "e.rs", "md", "f.md.bak"].map((path) => viewerOf(path)?.name)).toEqual([
      "markdown",
      "markdown",
      "markdown",
      undefined,
      undefined,
      undefined,
    ]);
  });
});
