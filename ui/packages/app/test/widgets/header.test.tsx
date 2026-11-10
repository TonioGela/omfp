/** L4: `omfp:header` over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import type { CallMethod } from "@omfp/client";
import type { ThreadSummary } from "@omfp/protocol";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent, { type UserEvent } from "@testing-library/user-event";
import { DELETE_THREAD, draftKey, SELECTED_THREAD } from "../../src/sdk/index.ts";
import { HeaderWidget } from "../../src/widgets/header/index.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";
import { type FakeClient, renderWithHost, testHost } from "../support/host.tsx";

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
  };
  for (const [method, result] of Object.entries(answers)) {
    client.answers.set(method as CallMethod, () => result);
  }
}

function setup(summary: ThreadSummary = thread("t1", "checkout", "w1", { tags: ["ci"] })) {
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
  // spec: UI-030
  test("without a selected thread the header says so", () => {
    renderWithHost(<HeaderWidget />);
    expect(screen.getByTestId("header-empty").textContent).toBe("No thread selected");
  });

  // spec: UI-030, UI-020
  test("it shows the title, tags, chips, process state and the session's model, thinking and context", async () => {
    setup();
    expect(screen.getByTestId("thread-title").textContent).toBe("checkout");
    expect(screen.getAllByTestId("thread-tag").map((tag) => tag.textContent)).toEqual(["ci"]);
    expect(screen.getAllByTestId("workspace-chip").map((chip) => chip.textContent)).toEqual(["⌂w1", "fe:main"]);
    expect(screen.getByTestId("thread-state").dataset.status).toBe("idle");
    await waitFor(() => expect(screen.getByTestId("context-usage").textContent).toBe("42% of 200k"));
    expect(within(screen.getByTestId("model-select")).getByRole("button").textContent).toContain("Opus (anthropic)");
    expect(within(screen.getByTestId("thinking-select")).getByRole("button").textContent).toContain("high");
  });

  // spec: UI-030, UI-023
  test("rename in place: Enter saves through threads.update, Escape cancels; unnamed threads start empty", async () => {
    const { client, user } = setup(thread("t1", "", "w1"));
    const title = screen.getByTestId("thread-title");
    expect([title.textContent, title.dataset.placeholder]).toEqual(["New thread", "true"]);
    await user.click(screen.getByTestId("rename-thread"));
    const input = screen.getByTestId("title-input");
    expect(document.activeElement).toBe(input);
    expect((input as HTMLInputElement).value).toBe("");
    await user.type(input, "checkout v2{Enter}");
    expect(client.callsTo("threads.update").map((call) => call.params)).toEqual([
      { threadId: "t1", title: "checkout v2" },
    ]);
    await user.click(screen.getByTestId("rename-thread"));
    await user.type(screen.getByTestId("title-input"), "x{Escape}");
    expect(screen.queryByTestId("title-input")).toBeNull();
    expect(client.callsTo("threads.update")).toHaveLength(1);
  });

  // spec: UI-030
  test("tags are edited as a comma-separated list", async () => {
    const { client, user } = setup();
    await user.click(screen.getByTestId("edit-tags"));
    const input = screen.getByTestId("tags-input");
    await user.clear(input);
    await user.type(input, "ci, release, ci{Enter}");
    expect(client.callsTo("threads.update").map((call) => call.params)).toEqual([
      { threadId: "t1", tags: ["ci", "release"] },
    ]);
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
});
