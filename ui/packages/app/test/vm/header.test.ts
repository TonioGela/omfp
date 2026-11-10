/** L1: the thread header's view-model (port of the Dioxus `vm/header.rs` tests). */

import { describe, expect, test } from "bun:test";
import { emptyThreadState, type ThreadState } from "@omfp/core";
import {
  decodeModels,
  decodeSession,
  decodeThinkingLevels,
  formatCost,
  formatTokens,
  headerView,
  NO_DETAILS,
  parseTags,
  sessionCost,
  splitModel,
  type ThreadDetails,
} from "../../src/vm/header.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";

const opus = {
  id: "opus",
  name: "Opus",
  api: "anthropic",
  provider: "anthropic",
  baseUrl: "",
  reasoning: true,
  input: ["text"],
  contextWindow: 200_000,
  maxTokens: 32000,
  cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
};

function details(): ThreadDetails {
  return {
    session: decodeSession({
      sessionId: "s",
      model: opus,
      thinkingLevel: "high",
      contextUsage: { tokens: 84000, contextWindow: 200_000, percent: 42.0 },
    }),
    models: decodeModels({ models: [{ ...opus, id: "haiku", name: "Haiku" }, { broken: true }] }),
    thinkingLevels: decodeThinkingLevels({ levels: ["off", "low", 3] }),
  };
}

function withCosts(...costs: number[]): ThreadState {
  const state = emptyThreadState();
  state.transcript = [
    { message_id: null, message: { role: "user", content: "hi" } },
    ...costs.map((total) => ({
      message_id: null,
      message: { role: "assistant" as const, content: [], usage: { cost: { total } } },
    })),
  ];
  return state;
}

describe("header", () => {
  // spec: UI-030, UI-011
  test("the header shows model, thinking, context and the workspace chips", () => {
    const workspaces = [workspace("w1", [repo("w1", "fe", "main")])];
    const view = headerView(thread("t", "checkout", "w1"), undefined, details(), workspaces);
    expect(view.model).toBe("anthropic/opus");
    expect(view.models).toEqual([
      { value: "anthropic/opus", label: "Opus (anthropic)" },
      { value: "anthropic/haiku", label: "Haiku (anthropic)" },
    ]);
    expect(view.thinking).toBe("high");
    expect(view.thinkingLevels).toEqual(["high", "off", "low"]);
    expect(view.context).toBe("42% of 200k");
    expect(view.chips.map((chip) => chip.label)).toEqual(["w1", "fe:main"]);
    expect(view.action).toBe("stop");
    expect(view.cost).toBeUndefined();
  });

  // spec: UI-030
  test("an unnamed thread shows the placeholder title; process state decides stop or restart", () => {
    const unnamed = headerView(thread("t", "", "w"), undefined, NO_DETAILS, []);
    expect([unnamed.title, unnamed.placeholder]).toEqual(["New thread", true]);
    const failed = headerView(
      thread("t", "x", "w", { process: { state: "failed", message: "boom" } }),
      undefined,
      NO_DETAILS,
      [],
    );
    expect([failed.status, failed.failure, failed.action, failed.placeholder]).toEqual([
      "failed",
      "boom",
      "restart",
      false,
    ]);
    const terminal = headerView(thread("t", "x", "w", { process: { state: "terminal" } }), undefined, NO_DETAILS, []);
    expect(terminal.action).toBeUndefined();
  });

  // spec: UI-026
  test("an archived thread's header offers unarchive", () => {
    expect(headerView(thread("t", "x", "w", { archivedAt: 4 }), undefined, NO_DETAILS, []).archived).toBe(true);
    expect(headerView(thread("t", "x", "w"), undefined, NO_DETAILS, []).archived).toBe(false);
  });

  // spec: UI-030
  test("cost sums the assistant usage of the transcript", () => {
    expect(sessionCost(withCosts())).toBe(0);
    expect(formatCost(sessionCost(withCosts(0.0125, 0.0125)))).toBe("$0.0250");
    expect(formatCost(12.5)).toBe("$12.50");
    expect(headerView(thread("t", "x", "w"), withCosts(0.5), NO_DETAILS, []).cost).toBe("$0.5000");
  });

  // spec: UI-030
  test("tags and model values are parsed from the editors", () => {
    expect(parseTags(" ci, release ,, ci")).toEqual(["ci", "release"]);
    expect(splitModel("openrouter/anthropic/opus")).toEqual({ provider: "openrouter", modelId: "anthropic/opus" });
    expect(splitModel("opus")).toBeUndefined();
    expect(splitModel("/opus")).toBeUndefined();
    expect(formatTokens(1_250_000)).toBe("1.2M");
    expect(formatTokens(950)).toBe("950");
  });

  // spec: UI-030
  test("malformed call results decode to nothing", () => {
    expect(decodeModels(null)).toEqual([]);
    expect(decodeThinkingLevels({ levels: "high" })).toEqual([]);
    expect(decodeSession("x")).toEqual({ model: undefined, thinkingLevel: undefined, contextUsage: undefined });
  });
});
