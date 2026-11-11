/** L4: the CodeMirror editor and merge view of the kit (S-13, UI-110, UI-111, UI-114), loaded lazily. */

import { afterEach, describe, expect, mock, test } from "bun:test";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { createRef } from "react";
import { CodeDiff, CodeEditor, type CodeEditorHandle } from "../src/index.ts";

afterEach(cleanup);

function content(root: HTMLElement): string {
  return [...root.querySelectorAll(".cm-line")].map((line) => line.textContent).join("\n");
}

describe("CodeEditor", () => {
  // spec: UI-110, UI-111
  test("shows the text with line numbers once its chunk loaded; a new value replaces it and counts as saved", async () => {
    const handle = createRef<CodeEditorHandle | null>() as { current: CodeEditorHandle | null };
    const dirty = mock((_dirty: boolean) => {});
    const { rerender } = render(
      <CodeEditor
        data-testid="editor"
        value={"fn one() {}\nfn two() {}\n"}
        path="src/lib.rs"
        readOnly
        handle={handle}
        onDirtyChange={dirty}
        loading={<p data-testid="loading">loading</p>}
        aria-label="Text of src/lib.rs"
      />,
    );
    const root = screen.getByTestId("editor");
    expect(root.dataset.readOnly).toBe("true");
    await waitFor(() => expect(content(root)).toBe("fn one() {}\nfn two() {}\n"));
    expect(root.querySelector(".cm-lineNumbers")).not.toBeNull();
    expect(root.querySelector(".cm-content")?.getAttribute("aria-label")).toBe("Text of src/lib.rs");
    expect(handle.current?.text()).toBe("fn one() {}\nfn two() {}\n");
    rerender(
      <CodeEditor
        data-testid="editor"
        value="changed on disk"
        path="src/lib.rs"
        readOnly
        handle={handle}
        onDirtyChange={dirty}
      />,
    );
    await waitFor(() => expect(content(root)).toBe("changed on disk"));
    expect(handle.current?.text()).toBe("changed on disk");
    expect(dirty).not.toHaveBeenCalledWith(true);
  });
});

describe("CodeDiff", () => {
  // spec: UI-114
  test("unified: the new text with the deleted lines inline; split: both sides", async () => {
    const { rerender } = render(
      <CodeDiff
        data-testid="diff"
        original={"a\nb\nc\n"}
        value={"a\nB\nc\n"}
        path="x.txt"
        layout="unified"
        aria-label="x.txt"
      />,
    );
    const root = screen.getByTestId("diff");
    await waitFor(() => expect(root.querySelector(".cm-deletedChunk")?.textContent).toContain("b"));
    expect(root.querySelector(".cm-changedLine")?.textContent).toBe("B");
    rerender(<CodeDiff data-testid="diff" original={"a\nb\nc\n"} value={"a\nB\nc\n"} path="x.txt" layout="split" />);
    await waitFor(() => expect(root.querySelectorAll(".cm-mergeViewEditor").length).toBe(2));
    expect(root.dataset.layout).toBe("split");
  });
});
