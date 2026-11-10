/** L4: `omfp:workspace` over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import type { CallMethod } from "@omfp/client";
import type { WorkspaceRepo } from "@omfp/protocol";
import { workspaceTopic } from "@omfp/protocol";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent, { type UserEvent } from "@testing-library/user-event";
import { ERRORS, SELECTED_THREAD } from "../../src/sdk/index.ts";
import { WorkspaceWidget } from "../../src/widgets/workspace/index.ts";
import { repo, repository, thread, workspace } from "../support/fixtures.ts";
import { renderWithHost, testHost } from "../support/host.tsx";

afterEach(cleanup);

const linked: WorkspaceRepo = { ...repo("w1", "be", "main"), kind: "link", target: "/src/be" };
const shared: WorkspaceRepo = { ...repo("w1", "fe", "feat"), sharedWith: ["w2"] };

function setup() {
  const base = testHost();
  for (const method of [
    "workspaces.add_repo",
    "workspaces.remove_repo",
    "workspaces.rescan",
    "workspaces.update",
    "workspaces.remove",
    "repos.add",
    "repos.remove",
  ] as const satisfies readonly CallMethod[]) {
    base.client.answers.set(method, () => ({}));
  }
  base.host.window.set(SELECTED_THREAD, "t1");
  const test = renderWithHost(<WorkspaceWidget />, base);
  test.client.connected();
  test.client.index({
    workspaces: [
      { ...workspace("w1", [shared, linked]), name: "checkout" },
      { ...workspace("w2"), name: "other" },
    ],
    threads: [thread("t1", "one", "w1"), thread("t2", "two", "w1"), thread("t3", "three", "w2")],
    repositories: [repository("r-fe", "fe")],
  });
  test.flush();
  return { ...test, user: userEvent.setup() };
}

function repos(): Array<[string | undefined, string | undefined]> {
  return screen.getAllByTestId("workspace-repo").map((row) => [row.dataset.name, row.dataset.kind]);
}

async function choose(user: UserEvent, testid: string, option: string): Promise<void> {
  await user.click(within(screen.getByTestId(testid)).getByRole("button"));
  await user.click(await screen.findByRole("option", { name: option }));
}

describe("omfp:workspace", () => {
  // spec: UI-025
  test("it shows the selected thread's workspace, its threads and repositories, with the git status live", () => {
    const { client, flush } = setup();
    expect(client.subscribed).toContain(workspaceTopic("w1"));
    expect(screen.getByTestId("workspace-name").textContent).toBe("checkout");
    expect(screen.getByTestId("workspace-path").textContent).toBe("/work/w1");
    expect(screen.getByTestId("shared-warning").textContent).toBe("shared by 2 threads");
    expect(screen.getAllByTestId("workspace-thread").map((button) => button.dataset.threadId)).toEqual(["t1", "t2"]);
    expect(repos()).toEqual([
      ["fe", "worktree"],
      ["be", "link"],
    ]);
    expect(screen.queryByTestId("git-status")).toBeNull();
    const [fe, be] = screen.getAllByTestId("workspace-repo");
    expect(within(fe ?? document.body).getByTestId("shared-repo").textContent).toBe("also in other");
    expect(within(be ?? document.body).getByTestId("repo-origin").textContent).toBe("→ /src/be");
    client.model.applySubscribed(workspaceTopic("w1"), {
      mode: "snapshot",
      epoch: "e1",
      seq: 0,
      state: [{ name: "fe", status: { branch: "feat", ahead: 1, behind: 0, dirty: true, files: [] } }],
    });
    client.notify();
    flush();
    expect(screen.getByTestId("git-status").textContent).toBe("feat · ↑1 ↓0 · dirty");
    expect(screen.getAllByTestId("workspace-repo")[0]?.dataset.dirty).toBe("true");
  });

  // spec: UI-025
  test("removing a link is immediate; anything else asks first", async () => {
    const { client, user } = setup();
    const [fe, be] = screen.getAllByTestId("workspace-repo");
    await user.click(within(be ?? document.body).getByTestId("remove-repo"));
    expect(client.callsTo("workspaces.remove_repo").map((call) => call.params)).toEqual([
      { workspaceId: "w1", name: "be" },
    ]);
    await user.click(within(fe ?? document.body).getByTestId("remove-repo"));
    expect(screen.getByTestId("remove-repo-dialog")).toBeDefined();
    await user.click(screen.getByTestId("confirm-remove-repo"));
    expect(client.callsTo("workspaces.remove_repo").map((call) => call.params)).toEqual([
      { workspaceId: "w1", name: "be" },
      { workspaceId: "w1", name: "fe" },
    ]);
  });

  // spec: UI-025
  test("add a link, a known repository, a worktree and a clone; rescan", async () => {
    const { client, user } = setup();
    await user.type(screen.getByTestId("link-repo-path"), "/tmp/x");
    await user.click(screen.getByTestId("link-repo-submit"));
    await choose(user, "known-repo-choice", "fe");
    await user.click(screen.getByTestId("add-known-repo-submit"));
    await user.click(screen.getByTestId("repository-new-worktree"));
    await user.type(screen.getByTestId("worktree-branch-name"), "feat/x");
    await user.click(screen.getByTestId("create-worktree-submit"));
    await user.type(screen.getByTestId("clone-repo-url"), "https://example.invalid/x.git");
    await user.click(screen.getByTestId("clone-repo-submit"));
    await user.click(screen.getByTestId("refresh-workspace"));
    expect(client.callsTo("workspaces.add_repo").map((call) => call.params)).toEqual([
      { workspaceId: "w1", source: { kind: "link", path: "/tmp/x" } },
      { workspaceId: "w1", source: { kind: "link", path: "/src/fe" } },
      { workspaceId: "w1", source: { kind: "worktree", repo: "r-fe", branch: "feat/x" } },
      { workspaceId: "w1", source: { kind: "clone", url: "https://example.invalid/x.git" } },
    ]);
    expect(client.callsTo("workspaces.rescan").map((call) => call.params)).toEqual([{ workspaceId: "w1" }]);
    await waitFor(() => expect((screen.getByTestId("link-repo-path") as HTMLInputElement).value).toBe(""));
  });

  // spec: UI-025
  test("the name is edited in place", async () => {
    const { client, user } = setup();
    await user.click(screen.getByTestId("rename-workspace"));
    const input = screen.getByTestId("workspace-name-input");
    await user.clear(input);
    await user.type(input, "checkout-v2{Enter}");
    expect(client.callsTo("workspaces.update").map((call) => call.params)).toEqual([
      { workspaceId: "w1", name: "checkout-v2" },
    ]);
  });

  // spec: UI-025
  test("all workspaces can be opened or removed; known repositories added or removed", async () => {
    const { client, user } = setup();
    const rows = screen.getAllByTestId("workspace-row");
    expect(rows.map((row) => within(row).getByTestId("workspace-thread-count").textContent)).toEqual([
      "2 threads",
      "1 thread",
    ]);
    await user.click(within(rows[1] ?? document.body).getByTestId("open-workspace"));
    expect(screen.getByTestId("workspace-current").dataset.workspaceId).toBe("w2");
    await user.click(within(rows[1] ?? document.body).getByTestId("remove-workspace"));
    await user.click(screen.getByTestId("confirm-remove-workspace"));
    expect(client.callsTo("workspaces.remove").map((call) => call.params)).toEqual([{ workspaceId: "w2" }]);
    await user.click(screen.getByTestId("remove-repository"));
    expect(screen.getByTestId("remove-repository-note").textContent).toBe("Its workspaces stay.");
    await user.click(screen.getByTestId("confirm-remove-repository"));
    expect(client.callsTo("repos.remove").map((call) => call.params)).toEqual([{ repositoryId: "r-fe" }]);
    await user.type(screen.getByTestId("add-repository-path"), "/src/api{Enter}");
    expect(client.callsTo("repos.add").map((call) => call.params)).toEqual([{ path: "/src/api" }]);
  });

  // spec: UI-025
  test("a failed action lands in the window's errors", async () => {
    const { client, host, user } = setup();
    client.answers.delete("workspaces.rescan");
    await user.click(screen.getByTestId("refresh-workspace"));
    act(() => client.callsTo("workspaces.rescan")[0]?.reject(new Error("git_error: boom")));
    await waitFor(() => expect(host.window.get(ERRORS)).toEqual(["git_error: boom"]));
  });
});
