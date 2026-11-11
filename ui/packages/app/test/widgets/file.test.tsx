/** L4: `omfp:file` and `useFile` over a fake client (UI-110..114, AGT-027). */

import { afterEach, describe, expect, test } from "bun:test";
import { CallError } from "@omfp/client";
import type { FileContent, GitStatus } from "@omfp/protocol";
import { workspaceTopic } from "@omfp/protocol";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type FilePoll, SELECTED_THREAD } from "../../src/sdk/index.ts";
import { FileWidget } from "../../src/widgets/file/index.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";
import { FakeClient, renderWithHost, testHost } from "../support/host.tsx";

afterEach(cleanup);

/** The host's file poll, run by the test. */
class ManualPoll {
  readonly checks = new Set<() => void>();
  readonly poll: FilePoll = (check) => {
    this.checks.add(check);
    return () => {
      this.checks.delete(check);
    };
  };

  run(): void {
    act(() => {
      for (const check of [...this.checks]) {
        check();
      }
    });
  }
}

const TEXT = "fn one() {}\nfn two() {}\n";

function text(content: string, extra: Partial<FileContent> = {}): FileContent {
  return {
    path: "fe/src/lib.rs",
    kind: "text",
    size: content.length,
    hash: `h-${content.length}`,
    unchanged: false,
    content,
    truncated: false,
    repo: "fe",
    head: "fn one() {}\n",
    ...extra,
  };
}

function setup(props: Record<string, unknown> = {}, first: FileContent = text(TEXT)) {
  const poll = new ManualPoll();
  const base = testHost(new FakeClient(), { filePoll: poll.poll });
  base.host.window.set(SELECTED_THREAD, "t1");
  let disk = first;
  base.client.answers.set("workspaces.read_file", (params) => {
    const { knownHash } = params as { knownHash?: string };
    return knownHash === disk.hash ? { ...disk, content: null, head: null, unchanged: true } : disk;
  });
  const test = renderWithHost(<FileWidget path="src/lib.rs" {...props} />, base);
  test.client.connected();
  test.client.index({
    workspaces: [workspace("w1", [repo("w1", "fe", "feat")])],
    threads: [thread("t1", "one", "w1")],
  });
  test.flush();
  return {
    ...test,
    poll,
    user: userEvent.setup(),
    /** What the file holds on disk from now on. */
    setDisk: (file: FileContent) => {
      disk = file;
    },
  };
}

function lines(element: HTMLElement): string {
  return [...element.querySelectorAll(".cm-line")].map((line) => line.textContent).join("\n");
}

async function editorText(): Promise<string> {
  const editor = await screen.findByTestId("file-editor");
  let shown = "";
  await waitFor(() => {
    shown = lines(editor);
    expect(editor.querySelector(".cm-content")).not.toBeNull();
  });
  return shown;
}

describe("omfp:file", () => {
  // spec: UI-110, AGT-027
  test("reads the file of the current workspace and shows it read-only with its repository", async () => {
    const test = setup();
    expect(test.client.callsTo("workspaces.read_file").map((call) => call.params)).toEqual([
      { workspaceId: "w1", path: "src/lib.rs" },
    ]);
    await waitFor(() => expect(screen.getByTestId("file").dataset.status).toBe("ready"));
    expect(screen.getByTestId("file-path").textContent).toBe("fe/src/lib.rs");
    expect(within(screen.getByTestId("file-bar")).getByText("fe")).toBeDefined();
    expect(await editorText()).toBe(TEXT);
    expect(screen.getByTestId("file-editor").dataset.readOnly).toBe("true");
    expect(screen.getByTestId("file-edit")).toBeDefined();
  });

  // spec: UI-110, AGT-027
  test("the poll asks with the known hash and reloads the view when the file changed; a git status change reads it again", async () => {
    const test = setup();
    await editorText();
    test.poll.run();
    const calls = test.client.callsTo("workspaces.read_file");
    expect(calls.at(-1)?.params).toEqual({ workspaceId: "w1", path: "src/lib.rs", knownHash: `h-${TEXT.length}` });
    expect(await editorText()).toBe(TEXT);
    test.setDisk(text("fn changed() {}\n"));
    test.poll.run();
    await waitFor(() => expect(lines(screen.getByTestId("file-editor"))).toBe("fn changed() {}\n"));
    const status: GitStatus = { branch: "feat", ahead: 0, behind: 0, dirty: true, files: [] };
    for (const [seq, dirty] of [[0, true] as const, [1, false] as const]) {
      test.client.model.applySubscribed(workspaceTopic("w1"), {
        mode: "snapshot",
        epoch: "e1",
        seq,
        state: [{ name: "fe", status: { ...status, dirty } }],
      });
      test.client.notify();
      test.flush();
    }
    // The first snapshot is no change; the second one reads the file again, `HEAD` included (no known hash).
    expect(test.client.callsTo("workspaces.read_file").at(-1)?.params).toEqual({
      workspaceId: "w1",
      path: "src/lib.rs",
    });
  });

  // spec: UI-111, AGT-027
  test("Edit switches to editing; Save writes with the hash the text was based on", async () => {
    const test = setup();
    await editorText();
    await test.user.click(screen.getByTestId("file-edit"));
    expect(screen.getByTestId("file-editor").dataset.readOnly).toBe("false");
    await test.user.click(screen.getByTestId("file-save"));
    const write = test.client.callsTo("workspaces.write_file");
    expect(write.map((call) => call.params)).toEqual([
      { workspaceId: "w1", path: "fe/src/lib.rs", content: TEXT, expectedHash: `h-${TEXT.length}` },
    ]);
    act(() => write[0]?.resolve({ path: "fe/src/lib.rs", hash: "h-saved", size: TEXT.length }));
    // Done goes back to read-only.
    await test.user.click(await screen.findByTestId("file-done"));
    await waitFor(() => expect(screen.getByTestId("file-editor").dataset.readOnly).toBe("true"));
  });

  // spec: UI-113, AGT-027
  test("a refused save shows the file on disk against the user's text; Discard mine reloads, nothing is written", async () => {
    const test = setup();
    await editorText();
    await test.user.click(screen.getByTestId("file-edit"));
    await test.user.click(screen.getByTestId("file-save"));
    test.setDisk(text("fn theirs() {}\n"));
    act(() =>
      test.client
        .callsTo("workspaces.write_file")[0]
        ?.reject(new CallError({ kind: "daemon", error: { code: "conflict", message: "changed on disk" } })),
    );
    const conflict = await screen.findByTestId("file-conflict");
    expect(screen.getByTestId("file-editor").hidden).toBe(true);
    const diff = within(conflict).getByTestId("file-conflict-diff");
    expect(diff.dataset.layout).toBe("split");
    await waitFor(() => expect(diff.querySelectorAll(".cm-mergeViewEditor").length).toBe(2));
    await waitFor(() => expect(lines(diff.querySelector(".cm-merge-a") as HTMLElement)).toBe("fn theirs() {}\n"));
    expect(lines(diff.querySelector(".cm-merge-b") as HTMLElement)).toBe(TEXT);
    await test.user.click(screen.getByTestId("file-discard"));
    expect(screen.queryByTestId("file-conflict")).toBeNull();
    await waitFor(() => expect(lines(screen.getByTestId("file-editor"))).toBe("fn theirs() {}\n"));
    expect(test.client.callsTo("workspaces.write_file")).toHaveLength(1);
  });

  // spec: UI-113
  test("Overwrite saves the user's text with the hash of the file now on disk", async () => {
    const test = setup();
    await editorText();
    await test.user.click(screen.getByTestId("file-edit"));
    await test.user.click(screen.getByTestId("file-save"));
    test.setDisk(text("theirs\n"));
    act(() =>
      test.client
        .callsTo("workspaces.write_file")[0]
        ?.reject(new CallError({ kind: "daemon", error: { code: "conflict", message: "changed" } })),
    );
    await screen.findByTestId("file-conflict");
    await waitFor(() =>
      expect(test.client.callsTo("workspaces.read_file").at(-1)?.params).toEqual({
        workspaceId: "w1",
        path: "src/lib.rs",
      }),
    );
    await test.user.click(screen.getByTestId("file-overwrite"));
    expect(test.client.callsTo("workspaces.write_file").at(-1)?.params).toEqual({
      workspaceId: "w1",
      path: "fe/src/lib.rs",
      content: TEXT,
      expectedHash: "h-7",
    });
  });

  // spec: UI-114
  test("diff shows the whole file against HEAD, unified first, then side by side", async () => {
    const test = setup({ diff: true });
    const diff = await screen.findByTestId("file-head-diff");
    expect(diff.dataset.layout).toBe("unified");
    await waitFor(() => expect(diff.querySelector(".cm-changedLine")?.textContent).toBe("fn two() {}"));
    await test.user.click(screen.getByTestId("file-layout"));
    expect(screen.getByTestId("file-head-diff").dataset.layout).toBe("split");
    expect(screen.queryByTestId("file-editor")).toBeNull();
  });

  // spec: UI-114
  test("a file HEAD lacks is all new; outside a repository there is nothing to compare with", async () => {
    setup({ diff: true }, text("new\n", { head: null }));
    expect((await screen.findByTestId("file-head-note")).textContent).toContain("Not in HEAD");
    cleanup();
    setup({ diff: true }, text("x\n", { head: null, repo: null }));
    expect((await screen.findByTestId("file-head-note")).textContent).toContain("Not in a repository");
  });

  // spec: UI-110
  test("images render, binaries are named, cut files say so and cannot be edited, missing files say so", async () => {
    setup({}, { ...text(""), kind: "image", content: "aGk=", mime: "image/png", head: null });
    expect((await screen.findByTestId("file-image")).querySelector("img")?.getAttribute("src")).toBe(
      "data:image/png;base64,aGk=",
    );
    cleanup();
    setup({}, { ...text(""), kind: "binary", content: null, size: 2048, head: null });
    expect((await screen.findByTestId("file-binary")).textContent).toBe("Binary file (2.0 KiB), not shown.");
    cleanup();
    setup({}, text("start\n", { truncated: true, size: 3 * 1024 * 1024, head: null }));
    expect(await screen.findByText(/Only the beginning of this 3\.0 MiB file/)).toBeDefined();
    expect(screen.queryByTestId("file-edit")).toBeNull();
    cleanup();
    const base = testHost(new FakeClient(), { filePoll: new ManualPoll().poll });
    base.host.window.set(SELECTED_THREAD, "t1");
    const test = renderWithHost(<FileWidget path="gone.rs" />, base);
    test.client.connected();
    test.client.index({ workspaces: [workspace("w1")], threads: [thread("t1", "one", "w1")] });
    test.flush();
    act(() =>
      test.client
        .callsTo("workspaces.read_file")[0]
        ?.reject(new CallError({ kind: "daemon", error: { code: "not_found", message: "no such file" } })),
    );
    expect((await screen.findByTestId("file-missing")).textContent).toBe("gone.rs does not exist.");
  });
});
