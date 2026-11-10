/** L1: the new-thread dialog's view-model (port of the Dioxus `vm/new_thread.rs` tests, without a title field). */

import { describe, expect, test } from "bun:test";
import { EMPTY_FORM, emptyChoice, type NewThreadForm, toRequest } from "../../src/vm/new-thread.ts";
import { repository } from "../support/fixtures.ts";

const repositories = [repository("r-fe", "fe")];

describe("new thread", () => {
  // spec: UI-023
  test("a thread in a new workspace without repositories is valid and has no title", () => {
    expect(toRequest(EMPTY_FORM, repositories)).toEqual({
      ok: true,
      request: { create: { workspace: { new: {} } }, repos: [], prompt: undefined },
    });
    const named: NewThreadForm = { ...EMPTY_FORM, workspace: { kind: "new", name: " api ", path: " /src/api " } };
    expect(toRequest(named, repositories)).toEqual({
      ok: true,
      request: { create: { workspace: { new: { name: "api", path: "/src/api" } } }, repos: [], prompt: undefined },
    });
  });

  // spec: UI-023
  test("an existing workspace must be chosen", () => {
    const form: NewThreadForm = { ...EMPTY_FORM, workspace: { kind: "existing", id: "" } };
    expect(toRequest(form, repositories)).toEqual({ ok: false, error: "Choose a workspace" });
    const chosen: NewThreadForm = { ...EMPTY_FORM, workspace: { kind: "existing", id: "w1" }, prompt: " Fix it " };
    expect(toRequest(chosen, repositories)).toEqual({
      ok: true,
      request: { create: { workspace: { id: "w1" } }, repos: [], prompt: "Fix it" },
    });
  });

  // spec: UI-023
  test("links, known repositories, worktrees and clones combine in order", () => {
    const form: NewThreadForm = {
      ...EMPTY_FORM,
      repos: [
        { kind: "worktree", repo: "/src/be", branch: "", baseRef: " main " },
        { kind: "link", path: "/tmp/scratch" },
        { kind: "known", repositoryId: "r-fe" },
        { kind: "clone", url: " https://example.invalid/x.git " },
      ],
    };
    const result = toRequest(form, repositories);
    expect(result.ok && result.request.repos).toEqual([
      { kind: "worktree", repo: "/src/be", baseRef: "main" },
      { kind: "link", path: "/tmp/scratch" },
      { kind: "link", path: "/src/fe" },
      { kind: "clone", url: "https://example.invalid/x.git" },
    ]);
  });

  // spec: UI-023
  test("incomplete or duplicate lines are refused", () => {
    const blank: NewThreadForm = { ...EMPTY_FORM, repos: [emptyChoice("worktree")] };
    expect(toRequest(blank, repositories)).toEqual({ ok: false, error: "Repository 1: enter a repository or path" });
    const unknown: NewThreadForm = { ...EMPTY_FORM, repos: [emptyChoice("known")] };
    expect(toRequest(unknown, repositories)).toEqual({ ok: false, error: "Repository 1: enter a known repository" });
    const twice: NewThreadForm = {
      ...EMPTY_FORM,
      repos: [
        { kind: "link", path: "/src/fe" },
        { kind: "known", repositoryId: "r-fe" },
      ],
    };
    expect(toRequest(twice, repositories)).toEqual({ ok: false, error: "Repository 2 is listed twice" });
  });
});
