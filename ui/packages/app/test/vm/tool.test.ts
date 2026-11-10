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
  test("a finished bash call renders through the fallback card", () => {
    const [card, ...rest] = cards(fixture("bash_tool_call"));
    expect(rest).toEqual([]);
    expect([card?.name, card?.status]).toEqual(["bash", "succeeded"]);
    expect(card?.summary).toContain("echo tool-output-123");
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
  test("while streaming the card shows the partial argument text", () => {
    const card = toolCard(
      "c1",
      { type: "toolCall", name: "bash", partialArgs: '{"command": "ec' },
      undefined,
      undefined,
      undefined,
    );
    expect(card.status).toBe("composing");
    expect(card.args).toBe('{"command": "ec');
    expect(card.summary).toBe('{"command": "ec');
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
    };
    const card = toolCard("c1", undefined, run, undefined, undefined);
    expect(expandedByDefault(card)).toBe(true);
    expect(card.summary).toBe("limit: 5, path: missing.txt");
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

  test("oneLine flattens whitespace and cuts with an ellipsis", () => {
    expect(oneLine("a\n  b\tc", 10)).toBe("a b c");
    expect(oneLine("abcdefghij", 5)).toBe("abcd…");
  });
});
