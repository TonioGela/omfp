import { describe, expect, test } from "bun:test";
import { noticeRow } from "../../src/vm/notice.ts";

describe("notice rows", () => {
  // spec: UI-043
  test("known notices are compact rows with their severity", () => {
    const retry = noticeRow({
      type: "auto_retry_start",
      data: { attempt: 2, maxAttempts: 5, delayMs: 1500, errorMessage: "overloaded" },
    });
    expect(retry).toEqual({
      kind: "retry",
      level: "warning",
      text: "Retrying (2/5) in 1.5 s: overloaded",
      json: undefined,
      urls: [],
    });
    const notice = noticeRow({
      type: "notice",
      data: { level: "error", message: "model switched", source: "advisor" },
    });
    expect([notice.level, notice.text]).toEqual(["error", "advisor: model switched"]);
    const compaction = noticeRow({
      type: "manual_compaction_end",
      data: { aborted: false, errorMessage: "too small" },
    });
    expect([compaction.level, compaction.text]).toEqual(["error", "Compaction failed: too small"]);
    const skipped = noticeRow({ type: "manual_compaction_end", data: { aborted: false, skipped: true } });
    expect(skipped.text).toBe("Compaction skipped");
  });

  // spec: UI-043
  test("unknown frames get a generic row with their JSON", () => {
    const row = noticeRow({ type: "unknown", data: { type: "irc_card", from: "advisor" } });
    expect([row.kind, row.text]).toEqual(["unknown", "Unrecognized irc_card frame"]);
    expect(row.json).toBe('{\n  "type": "irc_card",\n  "from": "advisor"\n}');
  });

  // spec: UI-345, UI-347
  test("a command's output and an open_url request are command cards with the URLs omp opened", () => {
    const dirs = noticeRow({ type: "command_output", data: { text: "Workspace roots:\n  /w/app" } });
    expect(dirs).toEqual({
      kind: "command",
      level: "info",
      text: "Workspace roots:\n  /w/app",
      json: undefined,
      urls: [],
    });
    const text =
      "━━━ OAuth Authorization Required ━━━\nOpening your browser to authorize. If it did not open:\nhttps://a.example/x";
    expect(noticeRow({ type: "command_output", data: { text } }).urls).toEqual(["https://a.example/x"]);
    const open = noticeRow({
      type: "open_url",
      data: { id: "1", url: "https://a.example/y", instructions: "Code: 42" },
    });
    expect([open.kind, open.text, open.urls]).toEqual(["open-url", "Code: 42", ["https://a.example/y"]]);
    expect(noticeRow({ type: "open_url", data: { id: "2", url: "https://a.example/z" } }).text).toBe(
      "omp asks to open this page in a browser",
    );
  });
});
