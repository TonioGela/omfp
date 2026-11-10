/** L4: `omfp:threads`, the left column, over a fake client (with the frame's delete dialogs). */

import { afterEach, describe, expect, test } from "bun:test";
import type { IndexSnapshot } from "@omfp/protocol";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DeleteThreadDialog, DeleteWorkspaceDialog } from "../../src/frame/dialogs.tsx";
import { DELETE_THREAD, DELETE_WORKSPACE, DRAFT_MESSAGE, DRAFT_THREAD, SELECTED_THREAD } from "../../src/sdk/index.ts";
import { COLLAPSED_WORKSPACES, ThreadsWidget } from "../../src/widgets/threads/index.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";
import { renderWithHost } from "../support/host.tsx";

afterEach(cleanup);

const INDEX: Partial<IndexSnapshot> = {
  workspaces: [
    { ...workspace("w1", [repo("w1", "fe", "main")]), name: "checkout" },
    { ...workspace("w2"), name: "scratch", owned: false, path: "/home/me/scratch" },
  ],
  threads: [
    thread("t1", "checkout-v2", "w1", { tags: ["release"], updatedAt: 5 }),
    thread("t2", "", "w2", { needsAttention: true, updatedAt: 5 }),
    thread("t3", "old", "w2", { archivedAt: 3 }),
  ],
};

function setup(index: Partial<IndexSnapshot> = INDEX) {
  const test = renderWithHost(
    <>
      <ThreadsWidget />
      <DeleteThreadDialog />
      <DeleteWorkspaceDialog />
    </>,
  );
  for (const method of ["threads.delete", "threads.unarchive", "workspaces.remove", "workspaces.update"] as const) {
    test.client.answers.set(method, () => ({}));
  }
  test.client.answers.set("fs.complete", () => ({ entries: [] }));
  test.client.answers.set("threads.search", () => ({ results: [] }));
  test.client.connected();
  test.client.index(index);
  test.flush();
  return { ...test, user: userEvent.setup() };
}

function rows(): string[] {
  return screen.queryAllByTestId("thread-row").map((row) => row.dataset.threadId ?? "");
}

function section(workspaceId: string): HTMLElement {
  const found = screen
    .getAllByTestId("workspace-section")
    .find((candidate) => candidate.dataset.workspaceId === workspaceId);
  if (found === undefined) {
    throw new Error(`no section ${workspaceId}`);
  }
  return found;
}

function sectionIds(): string[] {
  return screen.queryAllByTestId("workspace-section").map((candidate) => candidate.dataset.workspaceId ?? "");
}

describe("omfp:threads", () => {
  // spec: UI-020
  test("each workspace is a section with its threads (state badge, title), and no grouping, filter keys or settings", () => {
    setup();
    expect(sectionIds()).toEqual(["w1", "w2"]);
    expect(within(section("w1")).getByTestId("workspace-section-name").textContent).toBe("checkout");
    expect(rows()).toEqual(["t1", "t2"]);
    const [first, second] = screen.getAllByTestId("thread-row");
    expect(first?.dataset.status).toBe("idle");
    expect(within(first ?? document.body).getByTestId("thread-status")).toBeDefined();
    expect(second?.dataset.status).toBe("awaiting");
    expect(second?.textContent).toContain("New thread");
    expect(second?.dataset.titleSource).toBe("placeholder");
    for (const gone of ["grouping", "filter-key", "next-attention", "theme-mode", "density-mode", "workspace-chip"]) {
      expect(screen.queryByTestId(gone)).toBeNull();
    }
  });

  // spec: UI-020
  test("selecting a row highlights it and its workspace and marks it seen; a change elsewhere marks the other unread", async () => {
    const { client, flush, user } = setup();
    await user.click(screen.getAllByTestId("thread-row")[0] ?? document.body);
    expect(screen.getAllByTestId("thread-row")[0]?.getAttribute("aria-current")).toBe("true");
    expect(section("w1").dataset.current).toBe("true");
    expect(section("w2").dataset.current).toBeUndefined();
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

  // spec: UI-022
  test("the filter matches title, tags, workspace and repository names, hiding workspaces without a match", async () => {
    const { user } = setup();
    await user.type(screen.getByTestId("thread-filter"), "FE");
    expect(sectionIds()).toEqual(["w1"]);
    expect(rows()).toEqual(["t1"]);
    await user.clear(screen.getByTestId("thread-filter"));
    await user.type(screen.getByTestId("thread-filter"), "scratch");
    expect(sectionIds()).toEqual(["w2"]);
    expect(rows()).toEqual(["t2"]);
    await user.clear(screen.getByTestId("thread-filter"));
    await user.type(screen.getByTestId("thread-filter"), "nothing");
    expect(sectionIds()).toEqual([]);
    expect(screen.getByTestId("no-threads")).toBeDefined();
  });

  // spec: UI-022
  test("conversation matches come from threads.search once typing pauses, with their snippet", async () => {
    const { client, user } = setup();
    client.answers.set("threads.search", () => ({ results: [{ threadId: "t2", snippet: "…the retry loop…" }] }));
    await user.type(screen.getByTestId("thread-filter"), "retry");
    const snippet = await screen.findByTestId("thread-snippet");
    expect(snippet.textContent).toBe("…the retry loop…");
    expect(rows()).toEqual(["t2"]);
    expect(sectionIds()).toEqual(["w2"]);
    expect(client.callsTo("threads.search").map((call) => call.params)).toEqual([{ query: "retry" }]);
  });

  // spec: UI-022
  test("a section collapses with its chevron or `za`; the collapsed set is kept in window state", async () => {
    const { host, user } = setup();
    const chevron = within(section("w1")).getByTestId("workspace-collapse");
    expect(chevron.getAttribute("aria-expanded")).toBe("true");
    await user.click(chevron);
    expect(rows()).toEqual(["t2"]);
    expect(within(section("w1")).queryByTestId("add-thread")).toBeNull();
    expect(host.window.get(COLLAPSED_WORKSPACES)).toEqual(["w1"]);
    expect(within(section("w1")).getByTestId("workspace-collapse").getAttribute("aria-expanded")).toBe("false");
    act(() => within(section("w2")).getByTestId("workspace-menu").focus());
    act(() => {
      host.commands.run("nav.toggleSection", [], ["threads"]);
    });
    expect(host.window.get(COLLAPSED_WORKSPACES)).toEqual(["w1", "w2"]);
    expect(rows()).toEqual([]);
    // While filtering, collapsed sections show their matches.
    await user.type(screen.getByTestId("thread-filter"), "checkout");
    expect(rows()).toEqual(["t1"]);
  });

  // spec: UI-026
  test("archived threads sit behind a collapsed Archived (N) row: reopen, unarchive or delete them there", async () => {
    const { client, host, user } = setup();
    expect(within(section("w1")).queryByTestId("archived-toggle")).toBeNull();
    const toggle = within(section("w2")).getByTestId("archived-toggle");
    expect(toggle.textContent).toBe("Archived (1)");
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(rows()).toEqual(["t1", "t2"]);
    await user.click(toggle);
    expect(rows()).toEqual(["t1", "t2", "t3"]);
    const archived = within(screen.getByTestId("archived-list"));
    expect(archived.getByTestId("thread-row").dataset.archived).toBe("true");
    await user.click(archived.getByTestId("thread-row"));
    expect(host.window.get(SELECTED_THREAD)).toBe("t3");
    await user.click(archived.getByTestId("unarchive-thread"));
    expect(client.callsTo("threads.unarchive").map((call) => call.params)).toEqual([{ threadId: "t3" }]);
    await user.click(archived.getByTestId("row-delete-thread"));
    expect(host.window.get(DELETE_THREAD)).toBe("t3");
  });

  // spec: UI-023
  test("+ thread opens a draft thread in its workspace at once; leaving it empty discards it", async () => {
    const { client, host, user } = setup();
    await user.click(screen.getAllByTestId("thread-row")[0] ?? document.body);
    await user.click(within(section("w2")).getByTestId("add-thread"));
    expect(host.window.get(DRAFT_THREAD)).toEqual({ workspaceId: "w2" });
    expect(host.window.get(SELECTED_THREAD)).toBeUndefined();
    expect(section("w2").dataset.current).toBe("true");
    const draft = within(section("w2")).getByTestId("draft-thread");
    expect(draft.getAttribute("aria-current")).toBe("true");
    expect(client.callsTo("threads.create")).toHaveLength(0);
    await user.click(screen.getAllByTestId("thread-row")[0] ?? document.body);
    expect(host.window.get(DRAFT_THREAD)).toBeUndefined();
    expect(screen.queryByTestId("draft-thread")).toBeNull();
    // A draft with text stays listed while another thread is shown, and reopens from its row.
    await user.click(within(section("w1")).getByTestId("add-thread"));
    act(() => host.window.set(DRAFT_MESSAGE, { text: "fix it", images: [] }));
    await user.click(screen.getAllByTestId("thread-row")[1] ?? document.body);
    expect(host.window.get(SELECTED_THREAD)).toBe("t2");
    await user.click(within(section("w1")).getByTestId("draft-thread"));
    expect(host.window.get(SELECTED_THREAD)).toBeUndefined();
    expect(host.window.get(DRAFT_THREAD)?.workspaceId).toBe("w1");
  });

  // spec: UI-023
  test("+ workspace creates a workspace from an optional name, or an existing folder, and opens a draft in it", async () => {
    const { client, host, user } = setup();
    client.answers.set("workspaces.create", (params) => ({
      ...workspace("w9"),
      name: typeof params === "object" && params !== null && "name" in params ? String(params.name) : "workspace",
    }));
    await user.type(screen.getByTestId("new-workspace-name"), "billing{Enter}");
    expect(client.callsTo("workspaces.create").map((call) => call.params)).toEqual([{ name: "billing" }]);
    await waitFor(() => expect(host.window.get(DRAFT_THREAD)).toEqual({ workspaceId: "w9" }));
    expect(screen.getByTestId("new-workspace-name")).toHaveProperty("value", "");
    await user.click(screen.getByTestId("new-workspace-existing"));
    expect(screen.queryByTestId("new-workspace-name")).toBeNull();
    const path = screen.getByTestId("new-workspace-path");
    await waitFor(() => expect(document.activeElement).toBe(path));
    await user.type(path, "/srv/app/{Enter}");
    expect(client.callsTo("workspaces.create").map((call) => call.params)).toEqual([
      { name: "billing" },
      { path: "/srv/app" },
    ]);
    await waitFor(() => expect(screen.getByTestId("new-workspace-name")).toBeDefined());
  });

  // spec: UI-023
  test("a failed workspace creation shows the error and keeps the name", async () => {
    const { client, user } = setup();
    await user.type(screen.getByTestId("new-workspace-name"), "dup{Enter}");
    act(() => client.callsTo("workspaces.create")[0]?.reject(new Error("name taken")));
    expect((await screen.findByTestId("new-workspace-error")).textContent).toBe("name taken");
    expect(screen.getByTestId("new-workspace-name")).toHaveProperty("value", "dup");
  });

  // spec: UI-023
  test("with no workspace yet, the column shows only the focused + workspace field and a hint", async () => {
    setup({ workspaces: [], threads: [] });
    expect(screen.getByTestId("no-workspaces-hint").textContent).toContain("No workspace yet");
    expect(screen.queryByTestId("thread-filter")).toBeNull();
    expect(screen.queryByTestId("workspace-section")).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId("new-workspace-name")));
  });

  // spec: UI-020
  test("the section menu renames the workspace in place and opens its removal", async () => {
    const { client, host, user } = setup();
    await user.click(within(section("w1")).getByTestId("workspace-menu"));
    await user.click(await screen.findByTestId("workspace-rename"));
    const input = await screen.findByTestId("workspace-name-input");
    await user.clear(input);
    await user.type(input, "checkout-2{Enter}");
    expect(client.callsTo("workspaces.update").map((call) => call.params)).toEqual([
      { workspaceId: "w1", name: "checkout-2" },
    ]);
    await user.click(within(section("w2")).getByTestId("workspace-menu"));
    await user.click(await screen.findByTestId("workspace-remove"));
    expect(host.window.get(DELETE_WORKSPACE)).toBe("w2");
  });

  // spec: UI-020, UI-026
  test("a row's delete button asks first, then deletes the thread", async () => {
    const { client, host, user } = setup();
    act(() => host.window.set(SELECTED_THREAD, "t1"));
    const row = within(section("w1")).getByTestId("row-delete-thread");
    expect(row.getAttribute("aria-label")).toBe("Delete \u201ccheckout-v2\u201d…");
    await user.click(row);
    const dialog = await screen.findByTestId("delete-thread-dialog");
    await user.click(within(dialog).getByTestId("confirm-delete-thread"));
    expect(client.callsTo("threads.delete").map((call) => call.params)).toEqual([{ threadId: "t1" }]);
    expect(host.window.get(SELECTED_THREAD)).toBeUndefined();
  });

  // spec: UI-020, DMN-028
  test("deleting a workspace without threads asks, offering to delete its owned folder", async () => {
    const { client, host, user } = setup({
      ...INDEX,
      threads: [thread("t2", "", "w2")],
    });
    act(() => host.window.set(DRAFT_THREAD, { workspaceId: "w1" }));
    await user.click(within(section("w1")).getByTestId("delete-workspace"));
    const dialog = await screen.findByTestId("delete-workspace-dialog");
    expect(within(dialog).queryByTestId("delete-workspace-threads")).toBeNull();
    await user.click(within(dialog).getByTestId("delete-workspace-folder"));
    await user.click(within(dialog).getByTestId("confirm-delete-workspace"));
    await waitFor(() =>
      expect(client.callsTo("workspaces.remove").map((call) => call.params)).toEqual([
        { workspaceId: "w1", deleteFolder: true },
      ]),
    );
    expect(client.callsTo("threads.delete")).toHaveLength(0);
    expect(host.window.get(DRAFT_THREAD)).toBeUndefined();
  });

  // spec: UI-020, DMN-028
  test("deleting a workspace with threads lists them and deletes them first once asked to", async () => {
    const { client, host, user } = setup();
    act(() => within(section("w2")).getByTestId("workspace-collapse").focus());
    act(() => {
      host.commands.run("nav.delete", [], ["threads"]);
    });
    const dialog = await screen.findByTestId("delete-workspace-dialog");
    const listed = within(within(dialog).getByTestId("delete-workspace-thread-list")).getAllByRole("listitem");
    expect(listed.map((item) => item.dataset.threadId)).toEqual(["t2", "t3"]);
    // Not owned: its folder is kept, nothing to tick.
    expect(within(dialog).queryByTestId("delete-workspace-folder")).toBeNull();
    const confirm = within(dialog).getByTestId("confirm-delete-workspace");
    expect(confirm.hasAttribute("disabled") || confirm.getAttribute("aria-disabled") === "true").toBe(true);
    await user.click(within(dialog).getByTestId("delete-workspace-threads"));
    await user.click(within(dialog).getByTestId("confirm-delete-workspace"));
    await waitFor(() => expect(client.callsTo("workspaces.remove")).toHaveLength(1));
    expect(client.calls.map((call) => [call.method, call.params])).toEqual([
      ["threads.delete", { threadId: "t2" }],
      ["threads.delete", { threadId: "t3" }],
      ["workspaces.remove", { workspaceId: "w2" }],
    ]);
  });

  // spec: UI-020
  test("`dd` on a focused row asks to delete that thread", () => {
    const { host } = setup();
    act(() => screen.getAllByTestId("thread-row")[1]?.focus());
    act(() => {
      host.commands.run("nav.delete", [], ["threads"]);
    });
    expect(host.window.get(DELETE_THREAD)).toBe("t2");
  });

  // spec: UI-095, UI-022, UI-023
  test("nav commands move through the rows shown; `/` focuses the filter and `Space w` the + workspace field", () => {
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
    expect(document.activeElement).toBe(screen.getByTestId("thread-filter"));
    run("workspace.new");
    expect(document.activeElement).toBe(screen.getByTestId("new-workspace-name"));
  });

  // spec: UI-092
  test("every control and row is reachable with Tab", async () => {
    const { user } = setup();
    const reached = new Set<string>();
    for (let step = 0; step < 24; step += 1) {
      await user.tab();
      const testid = document.activeElement?.closest("[data-testid]")?.getAttribute("data-testid");
      if (testid !== undefined && testid !== null) {
        reached.add(testid);
      }
    }
    for (const testid of [
      "thread-filter",
      "new-workspace-name",
      "new-workspace-existing",
      "workspace-collapse",
      "workspace-menu",
      "delete-workspace",
      "thread-row",
      "row-delete-thread",
      "archived-toggle",
      "add-thread",
    ]) {
      expect(reached).toContain(testid);
    }
  });
});
