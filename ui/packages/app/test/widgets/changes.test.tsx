/** L4: `omfp:changes` over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import type { FileContent, GitStatus } from "@omfp/protocol";
import { workspaceTopic } from "@omfp/protocol";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type OmfpHost, OPEN_FILES, SELECTED_THREAD, type WidgetProps } from "../../src/sdk/index.ts";
import { ChangesWidget } from "../../src/widgets/changes/index.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";
import { renderWithHost, type TestHost, testHost } from "../support/host.tsx";
import { eventsUntil, fixtureEvents, isFrame } from "./chat/fixtures.ts";

afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.motion;
});

const PATCH = `diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,2 @@
 fn one() {}
-fn two() {}
+fn two() -> u8 { 2 }
`;

const STATUS: GitStatus = {
  branch: "feat",
  ahead: 1,
  behind: 0,
  dirty: true,
  files: [{ path: "src/lib.rs", oldPath: null, status: "modified", additions: 1, deletions: 1 }],
};

/** What `workspaces.read_file` answers: the file now (the patch's new side). */
let current: FileContent = {
  path: "fe/src/lib.rs",
  kind: "text",
  size: 33,
  hash: "h1",
  content: "fn one() {}\nfn two() -> u8 { 2 }\n",
  repo: "fe",
};

function setup(
  select: (host: OmfpHost) => void = (host) => host.window.set(SELECTED_THREAD, "t1"),
  props: WidgetProps = {},
) {
  const base = testHost();
  base.client.answers.set("workspaces.status", () => STATUS);
  base.client.answers.set("workspaces.rescan", () => ({}));
  base.client.answers.set("workspaces.diff", () => ({
    files: [{ path: "src/lib.rs", patch: PATCH, truncated: false }],
  }));
  base.client.answers.set("workspaces.read_file", () => current);
  select(base.host);
  const test = renderWithHost(<ChangesWidget {...props} />, base);
  test.client.connected();
  test.client.index({
    workspaces: [workspace("w1", [repo("w1", "fe", "feat"), repo("w1", "be", "main")]), workspace("w2")],
    threads: [thread("t1", "one", "w1"), thread("t2", "two", "w2")],
  });
  test.flush();
  return { ...test, user: userEvent.setup() };
}

function pushStatus(test: TestHost, status: GitStatus): void {
  test.client.model.applySubscribed(workspaceTopic("w1"), {
    mode: "snapshot",
    epoch: "e1",
    seq: 0,
    state: [
      { name: "fe", status },
      { name: "be", status: { branch: "main", ahead: 0, behind: 0, dirty: false, files: [] } },
    ],
  });
  test.client.notify();
  test.flush();
}

function repoSection(name: string): HTMLElement {
  const found = screen.getAllByTestId("changes-repo").find((element) => element.dataset.repo === name);
  if (found === undefined) {
    throw new Error(`no repository ${name}`);
  }
  return found;
}

describe("omfp:changes", () => {
  // spec: UI-080, DMN-026
  test("each repository of the current workspace lists its changed files with +/− counts from the workspace topic", () => {
    document.documentElement.dataset.motion = "none";
    const test = setup();
    expect(test.client.subscribed).toContain(workspaceTopic("w1"));
    expect(screen.getAllByTestId("changes-repo").map((element) => element.dataset.repo)).toEqual(["fe", "be"]);
    pushStatus(test, STATUS);
    const fe = repoSection("fe");
    expect(within(fe).getByTestId("changes-summary").textContent).toBe("1 file +1 −1");
    const file = within(fe).getByTestId("changes-file");
    expect([file.dataset.path, file.dataset.status]).toEqual(["src/lib.rs", "modified"]);
    expect(within(file).getByTestId("changes-counts").textContent).toBe("+1 −1");
    expect(within(repoSection("be")).getByTestId("changes-summary").textContent).toBe("clean");
    expect(within(repoSection("be")).queryByTestId("changes-file")).toBeNull();
  });

  // spec: UI-341
  test("the workspace folder's own repository has the workspace icon, mark and name; the repositories keep their look", () => {
    const base = testHost();
    base.host.window.set(SELECTED_THREAD, "t1");
    const test = renderWithHost(<ChangesWidget />, base);
    test.client.connected();
    test.client.index({
      workspaces: [workspace("w1", [{ ...repo("w1", "geo", "main"), isRoot: true }, repo("w1", "fe", "feat")])],
      threads: [thread("t1", "one", "w1")],
    });
    test.flush();
    const geo = repoSection("geo");
    const fe = repoSection("fe");
    expect([geo.dataset.workspace, geo.getAttribute("aria-label")]).toEqual(["true", "geo, the workspace folder"]);
    expect(within(geo).getByTestId("changes-workspace-mark").textContent).toBe("workspace");
    expect(geo.querySelectorAll(".omfp-changes__workspace-icon").length).toBe(1);
    expect([fe.dataset.workspace, fe.getAttribute("aria-label")]).toEqual([undefined, "fe"]);
    expect(within(fe).queryByTestId("changes-workspace-mark")).toBeNull();
    expect(fe.querySelectorAll(".omfp-changes__workspace-icon").length).toBe(0);
  });

  // spec: UI-080, UI-267
  test("with a repo prop it lists only that repository", () => {
    setup(undefined, { repo: "be" });
    expect(screen.getAllByTestId("changes-repo").map((element) => element.dataset.repo)).toEqual(["be"]);
  });

  // spec: UI-146, UI-165
  test("the +/− counts count up from 0 at motion full and show at once below it", async () => {
    document.documentElement.dataset.motion = "full";
    pushStatus(setup(), STATUS);
    const counts = () => within(repoSection("fe")).getByTestId("changes-counts").textContent;
    expect(counts()).toBe("+0 −0");
    await waitFor(() => expect(counts()).toBe("+1 −1"));
    cleanup();
    document.documentElement.dataset.motion = "reduced";
    pushStatus(setup(), STATUS);
    expect(counts()).toBe("+1 −1");
  });

  // spec: UI-080
  test("a file expands to the whole file against its text before the change, in the unified merge view", async () => {
    const test = setup();
    pushStatus(test, STATUS);
    await test.user.click(within(repoSection("fe")).getByTestId("changes-file-toggle"));
    expect(test.client.callsTo("workspaces.diff").map((call) => call.params)).toEqual([
      { workspaceId: "w1", repo: "fe", path: "src/lib.rs" },
    ]);
    expect(test.client.callsTo("workspaces.read_file").map((call) => call.params)).toEqual([
      { workspaceId: "w1", path: "fe/src/lib.rs" },
    ]);
    const diff = await screen.findByTestId("changes-diff");
    expect([diff.dataset.view, diff.dataset.layout, diff.getAttribute("aria-label")]).toEqual([
      "merge",
      "unified",
      "Diff of src/lib.rs",
    ]);
    await waitFor(() => expect(diff.querySelector(".cm-deletedChunk")?.textContent).toContain("fn two() {}"));
    expect(diff.querySelector(".cm-changedLine")?.textContent).toBe("fn two() -> u8 { 2 }");
  });

  // spec: UI-080, UI-090
  test("a file the patch does not fit (binary, changed since) shows the highlighted patch", async () => {
    current = { path: "fe/src/lib.rs", kind: "binary", size: 9, hash: "h2", repo: "fe" };
    try {
      const test = setup();
      pushStatus(test, STATUS);
      await test.user.click(within(repoSection("fe")).getByTestId("changes-file-toggle"));
      const diff = await screen.findByTestId("changes-diff");
      expect(diff.dataset.view).toBe("patch");
      expect([...diff.querySelectorAll("[data-kind=add]")].map((row) => row.textContent)).toEqual([
        "2+fn two() -> u8 { 2 }",
      ]);
      await waitFor(() => expect(diff.querySelector("[data-kind=add] .hljs-keyword")?.textContent).toBe("fn"));
    } finally {
      current = { ...current, kind: "text", content: "fn one() {}\nfn two() -> u8 { 2 }\n" };
    }
  });

  // spec: UI-116
  test("a changed file's path opens it in a file tab against HEAD, without expanding it", async () => {
    const test = setup();
    pushStatus(test, STATUS);
    const file = within(repoSection("fe")).getByTestId("changes-file");
    expect([file.dataset.omfpPath, file.dataset.omfpDiff]).toEqual(["fe/src/lib.rs", ""]);
    await test.user.click(within(file).getByTestId("changes-open"));
    expect(test.host.window.get(OPEN_FILES).w1?.tabs).toEqual([
      { path: "fe/src/lib.rs", line: undefined, diff: true, seq: 1 },
    ]);
    expect(test.client.callsTo("workspaces.diff")).toEqual([]);
  });

  // spec: UI-080
  test("the end of an edit, write or bash run of the selected thread refreshes the status", () => {
    const test = setup();
    test.client.thread("t1", eventsUntil("bash_tool_call", isFrame("tool_execution_end")));
    test.flush();
    expect(test.client.callsTo("workspaces.status")).toEqual([]);
    const events = fixtureEvents("bash_tool_call");
    const end = events.findIndex(isFrame("tool_execution_end"));
    test.client.threadEvents("t1", ...events.slice(end));
    test.flush();
    expect(test.client.callsTo("workspaces.status").map((call) => call.params)).toEqual([
      { workspaceId: "w1", repo: "fe" },
    ]);
  });

  // spec: UI-080
  test("the refresh button asks for a fresh status; without a workspace or repository there is nothing to list", async () => {
    const test = setup();
    await test.user.click(screen.getByTestId("changes-refresh"));
    expect(test.client.callsTo("workspaces.status").map((call) => call.params)).toEqual([
      { workspaceId: "w1", repo: "fe" },
    ]);
    cleanup();
    const other = setup((host) => host.window.set(SELECTED_THREAD, "t2"));
    expect(screen.getByTestId("changes-no-repo")).toBeDefined();
    await other.user.click(screen.getByTestId("changes-refresh"));
    expect(other.client.callsTo("workspaces.rescan").map((call) => call.params)).toEqual([{ workspaceId: "w2" }]);
    cleanup();
    setup(() => {});
    expect(screen.getByTestId("changes-none")).toBeDefined();
  });
});
