/** Hand-built protocol DTOs for view-model and widget tests (the TS twin of the Dioxus `test_support`). */

import type { Provenance, Repository, ThreadSummary, Workspace, WorkspaceRepo } from "@omfp/protocol";

export const provenance: Provenance = { createdBy: { kind: "user" }, client: "omfp/web" };

/** A known repository `name` at `/src/<name>`. */
export function repository(id: string, name: string): Repository {
  return { id, name, path: `/src/${name}`, defaultBaseRef: "main", remoteUrl: null, createdAt: 1 };
}

/** Entry `name` of the folder `/work/<workspace>`: a worktree of `/src/<name>` on `branch`. */
export function repo(workspace: string, name: string, branch: string | null): WorkspaceRepo {
  return {
    name,
    path: `/work/${workspace}/${name}`,
    kind: "worktree",
    main: `/src/${name}`,
    branch,
    head: null,
    isRoot: false,
    canonicalPath: `/work/${workspace}/${name}`,
    sharedWith: [],
  };
}

/** A folder `/work/<id>` named `id` holding `repos`. */
export function workspace(id: string, repos: WorkspaceRepo[] = [], threadIds: string[] = []): Workspace {
  return { id, name: id, path: `/work/${id}`, owned: true, repos, threadIds, provenance, createdAt: 1 };
}

/** An idle running thread in `workspaceId`. */
export function thread(
  id: string,
  title: string,
  workspaceId: string,
  extra: Partial<ThreadSummary> = {},
): ThreadSummary {
  return {
    id,
    title,
    titleSource: title === "" ? "placeholder" : "set",
    tags: [],
    workspaceId,
    process: { state: "running" },
    settled: true,
    needsAttention: false,
    provenance,
    createdAt: 1,
    updatedAt: 1,
    archivedAt: null,
    ...extra,
  };
}
