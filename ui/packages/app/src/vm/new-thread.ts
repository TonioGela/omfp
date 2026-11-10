/**
 * The new-thread dialog (UI-023): a workspace — a new one (optional name and existing folder) or an existing one —,
 * repositories to add right away (link a folder, link a known repository, new worktree, clone a URL; pull requests
 * after S-08) and the first message. There is no title field: the daemon names the thread (DMN-073).
 */

import type { CreateThreadParams, RepoSource, Repository, ThreadWorkspace } from "@omfp/protocol";
import { strings } from "../strings/index.ts";

const text = strings.shell.newThread;

export type WorkspaceChoice =
  | { readonly kind: "new"; readonly name: string; readonly path: string }
  | { readonly kind: "existing"; readonly id: string };

/** One repository line of the form; `kind` is also its `data-kind`. */
export type RepoChoice =
  | { readonly kind: "link"; readonly path: string }
  | { readonly kind: "known"; readonly repositoryId: string }
  | { readonly kind: "worktree"; readonly repo: string; readonly branch: string; readonly baseRef: string }
  | { readonly kind: "clone"; readonly url: string };

export type RepoChoiceKind = RepoChoice["kind"];

export interface NewThreadForm {
  readonly workspace: WorkspaceChoice;
  readonly repos: readonly RepoChoice[];
  readonly prompt: string;
}

export const EMPTY_FORM: NewThreadForm = { workspace: { kind: "new", name: "", path: "" }, repos: [], prompt: "" };

/** An empty line of `kind`. */
export function emptyChoice(kind: RepoChoiceKind): RepoChoice {
  switch (kind) {
    case "link":
      return { kind, path: "" };
    case "known":
      return { kind, repositoryId: "" };
    case "worktree":
      return { kind, repo: "", branch: "", baseRef: "" };
    case "clone":
      return { kind, url: "" };
  }
}

/** What the dialog asks the daemon: `threads.create`, then `workspaces.add_repo` per source, then the first message. */
export interface NewThreadRequest {
  readonly create: CreateThreadParams;
  readonly repos: readonly RepoSource[];
  readonly prompt: string | undefined;
}

export type Validated =
  | { readonly ok: true; readonly request: NewThreadRequest }
  | { readonly ok: false; readonly error: string };

/** The request, or the first problem with the form. */
export function toRequest(form: NewThreadForm, repositories: readonly Repository[]): Validated {
  const workspace = toWorkspace(form.workspace);
  if (workspace === undefined) {
    return { ok: false, error: text.needWorkspace };
  }
  const repos: RepoSource[] = [];
  for (const [index, choice] of form.repos.entries()) {
    const line = index + 1;
    const source = toSource(choice, repositories);
    if (typeof source === "string") {
      return { ok: false, error: text.missing(line, source) };
    }
    if (repos.some((known) => JSON.stringify(known) === JSON.stringify(source))) {
      return { ok: false, error: text.twice(line) };
    }
    repos.push(source);
  }
  return { ok: true, request: { create: { workspace }, repos, prompt: optional(form.prompt) } };
}

function toWorkspace(choice: WorkspaceChoice): ThreadWorkspace | undefined {
  if (choice.kind === "existing") {
    return choice.id === "" ? undefined : { id: choice.id };
  }
  const name = optional(choice.name);
  const path = optional(choice.path);
  return { new: { ...(name === undefined ? {} : { name }), ...(path === undefined ? {} : { path }) } };
}

/** The source of `choice`, or what is missing. */
function toSource(choice: RepoChoice, repositories: readonly Repository[]): RepoSource | string {
  switch (choice.kind) {
    case "link": {
      const path = optional(choice.path);
      return path === undefined ? text.whatLink : { kind: "link", path };
    }
    case "known": {
      const repository = repositories.find((candidate) => candidate.id === choice.repositoryId);
      return repository === undefined ? text.whatKnown : { kind: "link", path: repository.path };
    }
    case "worktree": {
      const repo = optional(choice.repo);
      if (repo === undefined) {
        return text.whatWorktree;
      }
      const branch = optional(choice.branch);
      const baseRef = optional(choice.baseRef);
      return {
        kind: "worktree",
        repo,
        ...(branch === undefined ? {} : { branch }),
        ...(baseRef === undefined ? {} : { baseRef }),
      };
    }
    case "clone": {
      const url = optional(choice.url);
      return url === undefined ? text.whatClone : { kind: "clone", url };
    }
  }
}

function optional(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed === "" ? undefined : trimmed;
}
