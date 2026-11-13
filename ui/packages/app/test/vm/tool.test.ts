import { describe, expect, test } from "bun:test";
import type { ThreadState, ToolRun } from "@omfp/core";
import { expandedByDefault, formatDuration, oneLine, type ToolCard, toolCard } from "../../src/vm/tool.ts";
import { items, messageView } from "../../src/vm/transcript.ts";
import { fixture, fixtureUntil, isFrame } from "../widgets/chat/fixtures.ts";

function cards(state: ThreadState): ToolCard[] {
  return items(state).flatMap((item) =>
    item.kind.type === "message"
      ? messageView(state, item.kind.index).blocks.flatMap((block) => (block.type === "tool" ? [block.card] : []))
      : [],
  );
}

describe("tool cards", () => {
  // spec: UI-050
  test("every card has the name, status, argument summary, arguments and result text", () => {
    const [card, ...rest] = cards(fixture("bash_tool_call"));
    expect(rest).toEqual([]);
    expect([card?.name, card?.status]).toEqual(["bash", "succeeded"]);
    expect(card?.summary).toBe("");
    expect(card?.args).toContain('"command"');
    expect(card?.result).toBe("tool-output-123\n");
    expect(card !== undefined && expandedByDefault(card)).toBe(false);
  });

  // spec: UI-050
  test("a running call shows its status", () => {
    const state = fixtureUntil("bash_tool_call", isFrame("tool_execution_end"));
    const run = state.tools[0];
    expect(run).toBeDefined();
    if (run !== undefined) {
      expect(toolCard(run.tool_call_id, undefined, run, undefined, undefined).status).toBe("running");
    }
  });

  // spec: UI-050
  test("while streaming the fallback card shows the partial argument text", () => {
    const card = toolCard(
      "c1",
      { type: "toolCall", name: "eval", partialArgs: '{"code": "1 +' },
      undefined,
      undefined,
      undefined,
    );
    expect(card.status).toBe("composing");
    expect(card.args).toBe('{"code": "1 +');
    expect(card.summary).toBe('{"code": "1 +');
  });

  // spec: UI-050
  test("while streaming a specialized card keeps the partial arguments out of its header", () => {
    const card = toolCard(
      "c1",
      { type: "toolCall", name: "write", partialArgs: '{"path": "a.md", "content": "Marker' },
      undefined,
      undefined,
      undefined,
    );
    expect(card.status).toBe("composing");
    expect(card.args).toBe('{"path": "a.md", "content": "Marker');
    expect(card.summary).toBe("");
  });

  // spec: UI-050
  test("a specialized card's header does not repeat its subject nor what its body shows", () => {
    const summary = (name: string, args: Record<string, string | number>) =>
      toolCard("c1", { type: "toolCall", name, arguments: args }, undefined, undefined, undefined).summary;
    expect(summary("write", { path: "repo/AGENTS.md", content: "Marker: one\nEdited.\n" })).toBe("");
    expect(summary("bash", { command: "git -C repo status --short", timeout: 30 })).toBe("timeout: 30");
    expect(summary("read", { path: "repo/AGENTS.md", limit: 5 })).toBe("limit: 5");
    expect(summary("edit", { path: "repo/a.rs", input: "[repo/a.rs#1A2B]\nPUT 1.=1:\n+x" })).toBe("");
    expect(summary("grep", { pattern: "Edited", path: "repo" })).toBe("path: repo");
    expect(summary("glob", { path: "src/**/*.ts" })).toBe("");
    expect(summary("web_search", { query: "omp rpc", limit: 3 })).toBe("limit: 3");
    // The ask card lists the form's questions.
    expect(summary("ask", { html: "<form><fieldset><legend>Which database?</legend></fieldset></form>" })).toBe("");
    // Tools without a subject keep every scalar argument.
    expect(summary("todo", { op: "done", task: "Edit AGENTS.md" })).toBe("op: done, task: Edit AGENTS.md");
  });

  // spec: UI-050, UI-052
  test("failed tools are expanded by default and take the error text", () => {
    const run: ToolRun = {
      tool_call_id: "c1",
      tool_name: "read",
      args: { path: "missing.txt", limit: 5, nested: { x: 1 } },
      intent: null,
      status: "failed",
      partial_result: null,
      preview: null,
      result: { content: [{ type: "text", text: "ENOENT" }] },
      background: null,
    };
    const card = toolCard("c1", undefined, run, undefined, undefined);
    expect(expandedByDefault(card)).toBe(true);
    expect(card.summary).toBe("limit: 5");
    expect(card.result).toBe("ENOENT");
  });

  // spec: UI-050
  test("duration runs from the calling message to the result", () => {
    const card = toolCard("c1", undefined, undefined, { role: "toolResult", timestamp: 3500 }, 1000);
    expect([card.status, card.durationMs]).toEqual(["succeeded", 2500]);
    expect(formatDuration(2500)).toBe("2.5 s");
    expect(formatDuration(125_000)).toBe("2 min 5 s");
    expect(formatDuration(40)).toBe("40 ms");
  });

  // spec: UI-680
  test("a negative duration shows as no time, never with a minus", () => {
    expect(formatDuration(-181)).toBe("0 ms");
    expect(formatDuration(-90_000)).toBe("0 ms");
    expect(formatDuration(0)).toBe("0 ms");
  });

  // spec: UI-472
  test("an omp run's result images are kept for the card; its text says [image] instead of the base64 JSON", () => {
    const run: ToolRun = {
      tool_call_id: "c1",
      tool_name: "eval",
      args: { language: "py", code: "plt.plot([1, 2])\nplt.show()" },
      intent: "Plotting the curve",
      status: "succeeded",
      partial_result: null,
      preview: null,
      result: { content: [{ type: "image", data: "R0lGOD", mimeType: "image/gif" }], details: {} },
      background: null,
    };
    const card = toolCard("c1", undefined, run, undefined, undefined);
    expect(card.images).toEqual([
      { mime: "image/gif", data: "R0lGOD", alt: "Image from eval plt.plot([1, 2]) plt.show()" },
    ]);
    expect(card.result).toBe("[image]");
    // Without a subject argument the alt text names the call by its summary (here its intent).
    const mcp = toolCard(
      "c1",
      undefined,
      { ...run, tool_name: "mcp_chart", args: { series: 3 } },
      undefined,
      undefined,
    );
    expect(mcp.images.map((image) => image.alt)).toEqual(["Image from mcp_chart Plotting the curve"]);
  });

  test("oneLine flattens whitespace and cuts with an ellipsis", () => {
    expect(oneLine("a\n  b\tc", 10)).toBe("a b c");
    expect(oneLine("abcdefghij", 5)).toBe("abcd…");
  });
});
