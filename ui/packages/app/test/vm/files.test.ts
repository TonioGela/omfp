/** L0: file tabs (UI-115) and the text a file had before a patch (UI-080). */

import { describe, expect, test } from "bun:test";
import {
  closeTab,
  decodeOpenFiles,
  type FileTabs,
  fileName,
  fileTabId,
  NO_FILE_TABS,
  openTab,
  originalFromPatch,
  workspaceFilePath,
} from "../../src/vm/files.ts";

describe("file tabs", () => {
  // spec: UI-115
  test("opening adds a selected tab at the end; opening it again selects it with the new line and diff", () => {
    let tabs = openTab(NO_FILE_TABS, "fe/src/a.rs", { line: 3 });
    tabs = openTab(tabs, "fe/src/b.rs", { diff: true });
    expect(tabs.tabs.map((tab) => tab.path)).toEqual(["fe/src/a.rs", "fe/src/b.rs"]);
    expect(tabs.active).toBe(fileTabId("fe/src/b.rs"));
    tabs = openTab(tabs, "fe/src/a.rs", { line: 9 });
    expect(tabs.tabs.map((tab) => tab.path)).toEqual(["fe/src/a.rs", "fe/src/b.rs"]);
    expect(tabs.tabs[0]).toEqual({ path: "fe/src/a.rs", line: 9, diff: undefined, seq: 2 });
    expect(tabs.active).toBe(fileTabId("fe/src/a.rs"));
  });

  // spec: UI-115
  test("closing the selected tab selects its neighbour, then the slot's own first tab", () => {
    let tabs: FileTabs = ["a", "b", "c"].reduce((open, path) => openTab(open, path), NO_FILE_TABS);
    tabs = closeTab({ ...tabs, active: fileTabId("b") }, "b");
    expect([tabs.tabs.map((tab) => tab.path), tabs.active]).toEqual([["a", "c"], fileTabId("a")]);
    tabs = closeTab(tabs, "c");
    expect([tabs.tabs.map((tab) => tab.path), tabs.active]).toEqual([["a"], fileTabId("a")]);
    tabs = closeTab(tabs, "a");
    expect(tabs).toEqual({ tabs: [], active: undefined });
    expect(closeTab(tabs, "gone")).toBe(tabs);
  });

  // spec: UI-115
  test("the persisted tabs decode leniently", () => {
    expect(
      decodeOpenFiles({
        w1: { tabs: [{ path: "a.rs", line: 2, diff: true }, { nope: 1 }, "x"], active: "file:a.rs" },
        w2: { tabs: "bad" },
        w3: null,
      }),
    ).toEqual({ w1: { tabs: [{ path: "a.rs", line: 2, diff: true, seq: 0 }], active: "file:a.rs" } });
    expect(decodeOpenFiles([])).toBeUndefined();
    expect(decodeOpenFiles("x")).toBeUndefined();
  });

  // spec: UI-115
  test("names and workspace paths", () => {
    expect(fileName("fe/src/lib.rs")).toBe("lib.rs");
    expect(fileName("Makefile")).toBe("Makefile");
    expect(workspaceFilePath({ name: "fe", isRoot: false }, "src/lib.rs")).toBe("fe/src/lib.rs");
    expect(workspaceFilePath({ name: "ws", isRoot: true }, "src/lib.rs")).toBe("src/lib.rs");
  });
});

const HEADER = "diff --git a/f b/f\n--- a/f\n+++ b/f\n";

describe("originalFromPatch", () => {
  // spec: UI-080
  test("a change in the middle, an addition at the end and a deletion at the start", () => {
    const current = "one\nTWO\nthree\nfour\nfive\n";
    const patch = `${HEADER}@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n@@ -4,1 +4,2 @@\n four\n+five\n`;
    expect(originalFromPatch(current, patch)).toBe("one\ntwo\nthree\nfour\n");
    const deleted = `${HEADER}@@ -1,2 +0,0 @@\n-zero\n-half\n`;
    expect(originalFromPatch("one\n", deleted)).toBe("zero\nhalf\none\n");
  });

  // spec: UI-080
  test("a new file came from nothing, a deleted one from its lines; no patch is no change", () => {
    expect(originalFromPatch("a\nb\n", `${HEADER}@@ -0,0 +1,2 @@\n+a\n+b\n`)).toBe("");
    expect(originalFromPatch("", `${HEADER}@@ -1,2 +0,0 @@\n-a\n-b\n`)).toBe("a\nb\n");
    expect(originalFromPatch("same\n", "")).toBe("same\n");
  });

  // spec: UI-080
  test("the end-of-file markers say which side lacked a final line break", () => {
    const added = `${HEADER}@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+a\n`;
    expect(originalFromPatch("a\n", added)).toBe("a");
    const removed = `${HEADER}@@ -1 +1 @@\n-a\n+a\n\\ No newline at end of file\n`;
    expect(originalFromPatch("a", removed)).toBe("a\n");
  });

  // spec: UI-080
  test("a patch that does not fit the file, or without a hunk (binary), gives nothing", () => {
    const patch = `${HEADER}@@ -1,2 +1,2 @@\n one\n-two\n+TWO\n`;
    expect(originalFromPatch("one\nchanged since\n", patch)).toBeUndefined();
    expect(originalFromPatch("x", `${HEADER}Binary files a/f and b/f differ\n`)).toBeUndefined();
    expect(originalFromPatch("one\n", `${HEADER}@@ -1,3 +5,3 @@\n a\n`)).toBeUndefined();
  });
});
