/** L4: `omfp:threads` over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, screen, within } from "@testing-library/react";
import userEvent, { type UserEvent } from "@testing-library/user-event";
import { COLOR_SCHEME, NEW_THREAD_DIALOG, SELECTED_THREAD } from "../../src/sdk/index.ts";
import { ThreadsWidget } from "../../src/widgets/threads/index.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";
import { renderWithHost } from "../support/host.tsx";

afterEach(cleanup);

function setup() {
  const test = renderWithHost(<ThreadsWidget />);
  test.client.connected();
  test.client.index({
    workspaces: [
      { ...workspace("w1", [repo("w1", "fe", "main")]), name: "checkout" },
      { ...workspace("w2"), name: "scratch" },
    ],
    threads: [
      thread("t1", "checkout-v2", "w1", { tags: ["release"], updatedAt: 5 }),
      thread("t2", "", "w2", { needsAttention: true, updatedAt: 5 }),
      thread("t3", "old", "w2", { archivedAt: 3 }),
    ],
  });
  test.flush();
  return { ...test, user: userEvent.setup() };
}

function rows(): string[] {
  return screen.queryAllByTestId("thread-row").map((row) => row.dataset.threadId ?? "");
}

async function choose(user: UserEvent, testid: string, option: string): Promise<void> {
  await user.click(within(screen.getByTestId(testid)).getByRole("button"));
  await user.click(await screen.findByRole("option", { name: option }));
}

describe("omfp:threads", () => {
  // spec: UI-020, UI-022
  test("rows show the state, the title (placeholder when unnamed) and the chips, grouped by workspace", () => {
    setup();
    expect(screen.getAllByTestId("thread-group").map((group) => group.dataset.group)).toEqual(["w1", "w2"]);
    expect(rows()).toEqual(["t1", "t2"]);
    const [first, second] = screen.getAllByTestId("thread-row");
    expect(first?.dataset.status).toBe("idle");
    expect(second?.dataset.status).toBe("awaiting");
    expect(second?.textContent).toContain("New thread");
    expect(second?.dataset.titleSource).toBe("placeholder");
    const chips = within(first ?? document.body).getAllByTestId("workspace-chip");
    expect(chips.map((chip) => [chip.dataset.kind, chip.textContent])).toEqual([
      ["workspace", "⌂checkout"],
      ["worktree", "fe:main"],
    ]);
  });

  // spec: UI-020
  test("selecting a row marks it current and seen; a change elsewhere marks the other thread unread", async () => {
    const { client, flush, host, user } = setup();
    await user.click(screen.getAllByTestId("thread-row")[0] ?? document.body);
    expect(host.window.get(SELECTED_THREAD)).toBe("t1");
    expect(screen.getAllByTestId("thread-row")[0]?.getAttribute("aria-current")).toBe("true");
    client.indexEvents(
      { kind: "thread_upserted", thread: thread("t1", "checkout-v2", "w1", { tags: ["release"], updatedAt: 9 }) },
      { kind: "thread_upserted", thread: thread("t2", "", "w2", { needsAttention: true, updatedAt: 9 }) },
    );
    flush();
    const [first, second] = screen.getAllByTestId("thread-row");
    expect(within(first ?? document.body).queryByTestId("unread")).toBeNull();
    expect(within(second ?? document.body).getByTestId("unread")).toBeDefined();
    await user.click(second ?? document.body);
    expect(within(second ?? document.body).queryByTestId("unread")).toBeNull();
  });

  // spec: UI-022, UI-026
  test("grouping, the key filter and the Archived filter change the list", async () => {
    const { user } = setup();
    await choose(user, "grouping", "tag");
    expect(screen.getAllByTestId("thread-group").map((group) => group.dataset.group)).toEqual(["release", ""]);
    await choose(user, "filter-key", "tag: release");
    expect(rows()).toEqual(["t1"]);
    await choose(user, "filter-key", "Archived");
    expect(rows()).toEqual(["t3"]);
    await choose(user, "filter-key", "all");
    expect(rows()).toEqual(["t1", "t2"]);
  });

  // spec: UI-022
  test("free text filters on title, tags and chips", async () => {
    const { user } = setup();
    await user.type(screen.getByTestId("filter-text"), "FE:MA");
    expect(rows()).toEqual(["t1"]);
    await user.clear(screen.getByTestId("filter-text"));
    await user.type(screen.getByTestId("filter-text"), "new threa");
    expect(rows()).toEqual(["t2"]);
  });

  // spec: UI-021, UI-023
  test("next waiting selects the thread awaiting input; + thread opens the dialog", async () => {
    const { host, user } = setup();
    expect(screen.getByTestId("next-attention").textContent).toBe("Next waiting (1)");
    await user.click(screen.getByTestId("next-attention"));
    expect(host.window.get(SELECTED_THREAD)).toBe("t2");
    await user.click(screen.getByTestId("new-thread"));
    expect(host.window.get(NEW_THREAD_DIALOG)).toBe(true);
  });

  // spec: UI-095
  test("nav commands move the selection in list order; nav.search focuses the search field", () => {
    const { host } = setup();
    const run = (name: string, scopes: string[] = []) =>
      act(() => {
        host.commands.run(name, [], scopes);
      });
    run("nav.next", ["threads"]);
    expect(host.window.get(SELECTED_THREAD)).toBe("t1");
    run("nav.next");
    expect(host.window.get(SELECTED_THREAD)).toBe("t2");
    run("nav.top");
    expect(host.window.get(SELECTED_THREAD)).toBe("t1");
    run("nav.bottom", ["threads"]);
    expect(host.window.get(SELECTED_THREAD)).toBe("t2");
    run("nav.halfUp");
    expect(host.window.get(SELECTED_THREAD)).toBe("t1");
    run("nav.search");
    expect(document.activeElement).toBe(screen.getByTestId("filter-text"));
  });

  // spec: UI-091, UI-094, UI-097
  test("the colour scheme picker sets this window's choice; the density picker sets the daemon's setting", async () => {
    const { client, host, user } = setup();
    client.answers.set("settings.set", () => ({ ui: { density: "normal" }, keymap: [], error: null }));
    await choose(user, "theme-mode", "Dark");
    await choose(user, "density-mode", "Normal");
    expect(host.window.get(COLOR_SCHEME)).toBe("dark");
    expect(client.callsTo("settings.set").map((call) => call.params)).toEqual([{ ui: { density: "normal" } }]);
  });

  // spec: UI-092
  test("every control and row is reachable with Tab", async () => {
    const { user } = setup();
    const reached = new Set<string>();
    for (let step = 0; step < 12; step += 1) {
      await user.tab();
      const element = document.activeElement;
      const testid = element?.closest("[data-testid]")?.getAttribute("data-testid");
      if (testid !== undefined && testid !== null) {
        reached.add(testid);
      }
    }
    for (const testid of [
      "new-thread",
      "next-attention",
      "grouping",
      "filter-key",
      "filter-text",
      "thread-row",
      "theme-mode",
    ]) {
      expect(reached).toContain(testid);
    }
  });
});
