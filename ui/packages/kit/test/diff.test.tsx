import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { Diff, parsePatch } from "../src/index.ts";

afterEach(cleanup);

const PATCH = `diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,5 @@ fn header()
 fn one() {}
-fn two() {}
+fn two() -> u8 { 2 }
+fn three() {}
\u0020
 fn four() {}
\\ No newline at end of file
`;

describe("Diff", () => {
  // spec: UI-080
  test("a unified patch splits into header, hunk and line rows with old and new line numbers", () => {
    expect(parsePatch(PATCH)).toEqual([
      { kind: "meta", text: "diff --git a/src/lib.rs b/src/lib.rs" },
      { kind: "meta", text: "index 1111111..2222222 100644" },
      { kind: "meta", text: "--- a/src/lib.rs" },
      { kind: "meta", text: "+++ b/src/lib.rs" },
      { kind: "hunk", text: "@@ -1,4 +1,5 @@ fn header()" },
      { kind: "context", text: "fn one() {}", oldLine: 1, newLine: 1 },
      { kind: "del", text: "fn two() {}", oldLine: 2 },
      { kind: "add", text: "fn two() -> u8 { 2 }", newLine: 2 },
      { kind: "add", text: "fn three() {}", newLine: 3 },
      { kind: "context", text: "", oldLine: 3, newLine: 4 },
      { kind: "context", text: "fn four() {}", oldLine: 4, newLine: 5 },
      { kind: "note", text: "\\ No newline at end of file" },
    ]);
    expect(parsePatch("")).toEqual([]);
  });

  // spec: UI-080
  test("renders one row per line, marking added and deleted lines", () => {
    render(<Diff patch={PATCH} aria-label="src/lib.rs" data-testid="diff" />);
    const diff = screen.getByTestId("diff");
    expect(diff.getAttribute("aria-label")).toBe("src/lib.rs");
    const rows = [...diff.querySelectorAll("[data-kind]")];
    expect(rows.map((row) => row.getAttribute("data-kind"))).toEqual([
      "hunk",
      "context",
      "del",
      "add",
      "add",
      "context",
      "context",
      "note",
    ]);
    const added = diff.querySelectorAll("[data-kind=add]");
    expect([...added].map((row) => row.textContent)).toEqual(["2+fn two() -> u8 { 2 }", "3+fn three() {}"]);
    expect(diff.querySelector("[data-kind=del]")?.textContent).toBe("2−fn two() {}");
  });
});
