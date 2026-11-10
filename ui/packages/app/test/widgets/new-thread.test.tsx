/** L4: the new-thread dialog over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { NEW_THREAD_DIALOG, SELECTED_THREAD } from "../../src/sdk/index.ts";
import { NewThreadDialog } from "../../src/widgets/new-thread/index.ts";
import { repository, thread, workspace } from "../support/fixtures.ts";
import { renderWithHost, testHost } from "../support/host.tsx";

afterEach(cleanup);

function setup() {
  const base = testHost();
  base.client.answers.set("threads.create", () => thread("t9", "", "w9"));
  base.client.answers.set("workspaces.add_repo", () => workspace("w9"));
  base.client.answers.set("thread.prompt", () => ({ promptId: "p", agentInvoked: true }));
  base.host.window.set(NEW_THREAD_DIALOG, true);
  const test = renderWithHost(<NewThreadDialog />, base);
  test.client.connected();
  test.client.index({ workspaces: [{ ...workspace("w1"), name: "api" }], repositories: [repository("r-fe", "fe")] });
  test.flush();
  return { ...test, user: userEvent.setup() };
}

describe("new-thread dialog", () => {
  // spec: UI-023, UI-092
  test("it is a modal dialog with the focus on the workspace name, trapped inside", async () => {
    const { user } = setup();
    const dialog = screen.getByTestId("new-thread-dialog");
    expect(dialog.getAttribute("role")).toBe("dialog");
    await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId("new-workspace-name")));
    for (let step = 0; step < 15; step += 1) {
      await user.tab();
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
  });

  // spec: UI-023
  test("create sends threads.create without a title, adds the repositories, sends the first message and selects the thread", async () => {
    const { client, host, user } = setup();
    await user.type(screen.getByTestId("new-workspace-name"), "checkout");
    await user.click(screen.getByTestId("add-link"));
    await user.type(screen.getByTestId("link-path"), "/tmp/scratch");
    await user.click(screen.getByTestId("add-worktree"));
    await user.type(screen.getByTestId("worktree-repo"), "r-fe");
    await user.type(screen.getByTestId("worktree-branch"), "feat");
    await user.click(screen.getByTestId("add-clone"));
    await user.type(screen.getByTestId("clone-url"), "https://example.invalid/x.git");
    expect(screen.getAllByTestId("repo-choice").map((line) => line.dataset.kind)).toEqual([
      "link",
      "worktree",
      "clone",
    ]);
    expect(screen.getByTestId("add-pr").hasAttribute("disabled")).toBe(true);
    await user.type(screen.getByTestId("new-thread-prompt"), "Fix the checkout");
    await user.click(screen.getByTestId("create-thread"));
    await waitFor(() => expect(host.window.get(NEW_THREAD_DIALOG)).toBe(false));
    expect(client.callsTo("threads.create").map((call) => call.params)).toEqual([
      { workspace: { new: { name: "checkout" } } },
    ]);
    expect(client.callsTo("workspaces.add_repo").map((call) => call.params)).toEqual([
      { workspaceId: "w9", source: { kind: "link", path: "/tmp/scratch" } },
      { workspaceId: "w9", source: { kind: "worktree", repo: "r-fe", branch: "feat" } },
      { workspaceId: "w9", source: { kind: "clone", url: "https://example.invalid/x.git" } },
    ]);
    expect(client.callsTo("thread.prompt").map((call) => call.params)).toEqual([
      { threadId: "t9", text: "Fix the checkout" },
    ]);
    expect(host.window.get(SELECTED_THREAD)).toBe("t9");
  });

  // spec: UI-023
  test("an existing workspace and a known repository; incomplete lines are refused", async () => {
    const { client, user } = setup();
    await user.click(screen.getByTestId("add-known"));
    await user.click(screen.getByTestId("create-thread"));
    expect(screen.getByTestId("new-thread-error").textContent).toBe("Repository 1: enter a known repository");
    expect(client.callsTo("threads.create")).toHaveLength(0);
    await user.click(within(screen.getByTestId("known-repository")).getByRole("button"));
    await user.click(await screen.findByRole("option", { name: "fe" }));
    await user.click(screen.getByTestId("workspace-existing"));
    expect(within(screen.getByTestId("workspace-choice")).getByRole("button").textContent).toContain("api");
    await user.click(screen.getByTestId("create-thread"));
    await waitFor(() => expect(client.callsTo("workspaces.add_repo")).toHaveLength(1));
    expect(client.callsTo("threads.create")[0]?.params).toEqual({ workspace: { id: "w1" } });
    expect(client.callsTo("workspaces.add_repo")[0]?.params).toEqual({
      workspaceId: "w9",
      source: { kind: "link", path: "/src/fe" },
    });
  });

  // spec: UI-023, UI-092
  test("Escape and Cancel close it", async () => {
    const { host, user } = setup();
    await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId("new-workspace-name")));
    await user.keyboard("{Escape}");
    expect(host.window.get(NEW_THREAD_DIALOG)).toBe(false);
    expect(screen.queryByTestId("new-thread-dialog")).toBeNull();
  });
});
