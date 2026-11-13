/** L4: the CodeMirror editor and merge view of the kit (S-13, UI-110, UI-111, UI-114), loaded lazily. */

import { afterEach, describe, expect, mock, test } from "bun:test";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Activity, createRef } from "react";
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

  // spec: UI-403
  test("hidden and shown again (its effects cleaned up and run again) it keeps the user's text, unsaved, and the focus where it went", async () => {
    const handle = createRef<CodeEditorHandle | null>() as { current: CodeEditorHandle | null };
    const dirty = mock((_dirty: boolean) => {});
    const editor = (mode: "visible" | "hidden", value: string) => (
      <>
        <button type="button" data-testid="elsewhere">
          elsewhere
        </button>
        <Activity mode={mode}>
          <CodeEditor
            data-testid="editor"
            value={value}
            path="src/lib.rs"
            readOnly={false}
            handle={handle}
            onDirtyChange={dirty}
          />
        </Activity>
      </>
    );
    const { rerender } = render(editor("visible", "fn one() {}"));
    const root = screen.getByTestId("editor");
    await waitFor(() => expect(content(root)).toBe("fn one() {}"));
    const user = userEvent.setup();
    // Editing starts in insert mode with the focus in the editor (UI-111).
    await user.keyboard("XY");
    expect(content(root)).toBe("XYfn one() {}");
    expect(dirty.mock.calls.at(-1)).toEqual([true]);
    rerender(editor("hidden", "fn one() {}"));
    screen.getByTestId("elsewhere").focus();
    rerender(editor("visible", "fn one() {}"));
    await waitFor(() => expect(root.querySelector(".cm-content")).not.toBeNull());
    expect(content(root)).toBe("XYfn one() {}");
    expect(handle.current?.text()).toBe("XYfn one() {}");
    expect(dirty.mock.calls.at(-1)).toEqual([true]);
    expect(document.activeElement?.getAttribute("data-testid")).toBe("elsewhere");
    // A new value while hidden still replaces the text once shown.
    rerender(editor("hidden", "fn two() {}"));
    rerender(editor("visible", "fn two() {}"));
    await waitFor(() => expect(content(root)).toBe("fn two() {}"));
    expect(dirty.mock.calls.at(-1)).toEqual([false]);
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
