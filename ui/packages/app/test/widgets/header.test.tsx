/** L4: `omfp:header` over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import type { CallMethod } from "@omfp/client";
import type { ThreadSummary } from "@omfp/protocol";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent, { type UserEvent } from "@testing-library/user-event";
import { DELETE_THREAD, draftKey, REVEAL_WIDGET, SELECTED_THREAD } from "../../src/sdk/index.ts";
import { HeaderWidget } from "../../src/widgets/header/index.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";
import { type FakeClient, renderWithHost, testHost } from "../support/host.tsx";
import { installResizeObserver, resize } from "../support/resize.ts";
import { fixtureEvents, isFrame } from "./chat/fixtures.ts";

afterEach(cleanup);

const opus = { id: "opus", name: "Opus", provider: "anthropic" };

function answer(client: FakeClient): void {
  const answers: Partial<Record<CallMethod, unknown>> = {
    "thread.state": {
      sessionId: "s",
      model: opus,
      thinkingLevel: "high",
      contextUsage: { tokens: 1, contextWindow: 200_000, percent: 42 },
    },
    "thread.models": { models: [opus, { id: "haiku", name: "Haiku", provider: "anthropic" }] },
    "thread.thinking_levels": { levels: ["off", "high"] },
    "threads.update": {},
    "threads.archive": {},
    "threads.unarchive": {},
    "thread.stop": {},
    "thread.restart": {},
    "thread.set_model": {},
    "thread.set_thinking": {},
    "thread.subagents": { subagents: [] },
  };
  for (const [method, result] of Object.entries(answers)) {
    client.answers.set(method as CallMethod, () => result);
  }
}

function setup(summary: ThreadSummary = thread("t1", "checkout", "w1")) {
  const base = testHost();
  answer(base.client);
  base.host.window.set(SELECTED_THREAD, summary.id);
  const test = renderWithHost(<HeaderWidget />, base);
  test.client.connected();
  test.client.index({ workspaces: [workspace("w1", [repo("w1", "fe", "main")])], threads: [summary] });
  test.flush();
  return { ...test, user: userEvent.setup() };
}

async function choose(user: UserEvent, testid: string, option: string): Promise<void> {
  await user.click(within(screen.getByTestId(testid)).getByRole("button"));
  await user.click(await screen.findByRole("option", { name: option }));
}

describe("omfp:header", () => {
  // spec: UI-274
  test("a badge counts what runs in the background and shows omfp:background", async () => {
    const test = setup();
    expect(screen.queryByTestId("background-badge")).toBeNull();
    const events = fixtureEvents("async_bash_tool_call");
    const ended = events.findIndex(isFrame("tool_execution_end")) + 1;
    test.client.thread("t1", events.slice(0, ended));
    test.flush();
    const badge = screen.getByTestId("background-badge");
    expect(badge.textContent).toBe("1 in background");
    await test.user.click(badge);
    expect(test.host.window.get(REVEAL_WIDGET)?.reference).toBe("omfp:background");
    test.client.threadEvents("t1", ...events.slice(ended));
    test.flush();
    expect(screen.queryByTestId("background-badge")).toBeNull();
  });

  // spec: UI-340
  test("one workspace chip: name and repository count, the repositories as its tooltip; a press shows omfp:workspace", async () => {
    const test = setup();
    const chip = screen.getByTestId("workspace-chip");
    expect([chip.textContent, chip.dataset.repos]).toEqual(["w1 · 1 repo", "1"]);
    const label = chip.querySelector("[title]");
    expect(label?.getAttribute("title")).toBe("fe:main (worktree)");
    const description = document.getElementById(chip.getAttribute("aria-describedby") ?? "");
    expect(description?.textContent).toBe("fe:main (worktree)");
    await test.user.click(chip);
    expect(test.host.window.get(REVEAL_WIDGET)?.reference).toBe("omfp:workspace");
  });

  // spec: UI-030
  test("without a selected thread the header says so", () => {
    renderWithHost(<HeaderWidget />);
    expect(screen.getByTestId("header-empty").textContent).toBe("No thread selected");
  });

  // spec: UI-030, UI-020
  test("it shows the title, chips, process state and the session's model, thinking and context; no tags", async () => {
    setup();
    expect(screen.getByTestId("thread-title").textContent).toBe("checkout");
    expect(screen.queryByTestId("edit-tags")).toBeNull();
    expect(screen.getAllByTestId("workspace-chip").map((chip) => chip.textContent)).toEqual(["w1 · 1 repo"]);
    expect(screen.getByTestId("thread-state").dataset.status).toBe("idle");
    await waitFor(() => expect(screen.getByTestId("context-usage").textContent).toBe("84k/200k"));
    expect(screen.getByTestId("context-usage").getAttribute("aria-label")).toBe("Context window usage: 42% of 200k");
    expect(within(screen.getByTestId("model-select")).getByRole("button").textContent).toContain("Opus (anthropic)");
    expect(within(screen.getByTestId("thinking-select")).getByRole("button").textContent).toContain("high");
  });

  // spec: UI-202
  test("the full header's title carries the full title as its tooltip, as the compact one does", () => {
    const long = "a thread title long enough to be cut with an ellipsis in the full header";
    setup(thread("t1", long, "w1"));
    expect(screen.getByTestId("thread-title").getAttribute("title")).toBe(long);
  });

  // spec: UI-030, UI-023, UI-092, UI-286
  test("rename in place: Enter saves through threads.update, Escape cancels, both back on the title; unnamed threads start empty", async () => {
    const { client, user } = setup(thread("t1", "", "w1"));
    const title = screen.getByTestId("thread-title");
    expect([title.textContent, title.dataset.placeholder]).toEqual(["New thread", "true"]);
    await user.click(screen.getByTestId("rename-thread"));
    const input = screen.getByTestId("title-input");
    expect(document.activeElement === input).toBe(true);
    expect((input as HTMLInputElement).value).toBe("");
    await user.type(input, "checkout v2{Enter}");
    expect(client.callsTo("threads.update").map((call) => call.params)).toEqual([
      { threadId: "t1", title: "checkout v2" },
    ]);
    // The focus stays on the renamed item: its title.
    expect(document.activeElement?.getAttribute("data-testid")).toBe("thread-title");
    await user.click(screen.getByTestId("rename-thread"));
    await user.type(screen.getByTestId("title-input"), "x{Escape}");
    expect(screen.queryByTestId("title-input")).toBeNull();
    expect(document.activeElement?.getAttribute("data-testid")).toBe("thread-title");
    expect(client.callsTo("threads.update")).toHaveLength(1);
  });

  // spec: UI-030
  test("choosing a model or thinking level sets it and refreshes the session", async () => {
    const { client, user } = setup();
    await waitFor(() => expect(screen.getByTestId("context-usage")).toBeDefined());
    await choose(user, "model-select", "Haiku (anthropic)");
    expect(client.callsTo("thread.set_model").map((call) => call.params)).toEqual([
      { threadId: "t1", provider: "anthropic", modelId: "haiku" },
    ]);
    await choose(user, "thinking-select", "off");
    expect(client.callsTo("thread.set_thinking").map((call) => call.params)).toEqual([
      { threadId: "t1", level: "off" },
    ]);
    await waitFor(() => expect(client.callsTo("thread.state")).toHaveLength(3));
  });

  // spec: UI-030
  test("stop for a running process, restart with the failure for a failed one", async () => {
    const { client, user } = setup();
    expect(screen.getByTestId("process-action").textContent).toBe("Stop");
    await user.click(screen.getByTestId("process-action"));
    expect(client.callsTo("thread.stop")).toHaveLength(1);
    cleanup();
    const failed = setup(thread("t1", "x", "w1", { process: { state: "failed", message: "omp exited" } }));
    expect(screen.getByTestId("thread-failure").textContent).toBe("omp exited");
    await failed.user.click(screen.getByTestId("process-action"));
    expect(failed.client.callsTo("thread.restart")).toHaveLength(1);
  });

  // spec: UI-026
  test("archiving the selected thread clears the selection and its draft; delete asks first", async () => {
    const { client, host, user } = setup();
    host.window.set(draftKey("t1"), { text: "half", images: [] });
    await user.click(screen.getByTestId("delete-thread"));
    expect(host.window.get(DELETE_THREAD)).toBe("t1");
    expect(client.callsTo("threads.delete")).toHaveLength(0);
    await user.click(screen.getByTestId("archive-thread"));
    expect(client.callsTo("threads.archive").map((call) => call.params)).toEqual([{ threadId: "t1" }]);
    expect(host.window.get(SELECTED_THREAD)).toBeUndefined();
    expect(host.window.get(draftKey("t1")).text).toBe("");
    expect(host.window.get(DELETE_THREAD)).toBeUndefined();
  });

  // spec: UI-026
  test("an archived thread offers unarchive", async () => {
    const { client, user } = setup(thread("t1", "x", "w1", { archivedAt: 2 }));
    expect(screen.getByTestId("archive-thread").textContent).toBe("Unarchive");
    await user.click(screen.getByTestId("archive-thread"));
    expect(client.callsTo("threads.unarchive")).toHaveLength(1);
  });

  // spec: UI-202
  test("narrower than 48rem the header is one row: title, state, model and a ⋯ popover with the rest", async () => {
    const restore = installResizeObserver();
    try {
      const { client, user } = setup();
      const header = screen.getByTestId("thread-header");
      expect(screen.queryAllByTestId("header-more").length).toBe(0);
      resize(header, 600);
      expect(header.dataset.compact).toBe("true");
      const inHeader = (testid: string) => within(header).queryAllByTestId(testid).length;
      expect(
        ["thread-title", "thread-state", "model-select", "header-more", "process-action", "archive-thread"].map(
          inHeader,
        ),
      ).toEqual([1, 1, 1, 1, 0, 0]);
      expect(["thinking-select", "workspace-chip", "rename-thread"].map(inHeader)).toEqual([0, 0, 0]);
      await user.click(screen.getByTestId("header-more"));
      const more = await screen.findByTestId("header-more-dialog");
      expect(
        ["rename-thread", "process-action", "archive-thread", "delete-thread", "thinking-select"].map(
          (testid) => within(more).queryAllByTestId(testid).length,
        ),
      ).toEqual([1, 1, 1, 1, 1]);
      expect(
        within(more)
          .getAllByTestId("workspace-chip")
          .map((chip) => chip.textContent),
      ).toEqual(["w1 · 1 repo"]);
      await user.click(within(more).getByTestId("rename-thread"));
      expect(screen.queryAllByTestId("header-more-dialog").length).toBe(0);
      expect(within(header).queryAllByTestId("title-input").length).toBe(1);
      await user.keyboard("{Escape}");
      resize(header, 1000);
      expect(header.dataset.compact).toBeUndefined();
      expect(inHeader("process-action")).toBe(1);
      resize(header, 600);
      await user.click(screen.getByTestId("header-more"));
      await user.click(within(await screen.findByTestId("header-more-dialog")).getByTestId("archive-thread"));
      expect(client.callsTo("threads.archive").map((call) => call.params)).toEqual([{ threadId: "t1" }]);
    } finally {
      restore();
    }
  });
});
