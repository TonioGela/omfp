/**
 * `omfp:workspace` (UI-025): the selected thread's workspace — name (editable), folder, threads, and its repositories
 * with kind, origin, branch, dirty state and ahead/behind, live from `workspace/<id>` —; actions add (link a folder,
 * link a known repository, new worktree, clone) and remove (with a confirmation for anything but a link, DMN-025),
 * rescan; badges when the workspace is shared by several threads or a checkout is in another workspace with live
 * threads (DMN-027). Below: every workspace (threads count, open, remove) and the known repositories (add, remove,
 * new worktree in this workspace).
 */

import { Badge, Button, ConfirmDialog, type Key, Select, TextField } from "@omfp/kit";
import type { RepoSource, Repository, Workspace } from "@omfp/protocol";
import { type FormEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import {
  reportError,
  SELECTED_THREAD,
  useCall,
  useHost,
  useRepositories,
  useThreadActions,
  useThreadSummary,
  useThreads,
  useWindowState,
  useWorkspace,
  useWorkspaces,
  type WidgetProps,
  windowKey,
} from "../../sdk/index.ts";
import { strings } from "../../strings/index.ts";
import {
  type RepoRow,
  type RepositoryRow,
  removalNeedsConfirmation,
  repositoryRows,
  type WorkspaceRow,
  workspaceList,
  workspaceRow,
} from "../../vm/workspaces.ts";

const text = strings.shell.workspace;

/** A workspace opened from the list; the selected thread's workspace otherwise. */
export const VIEWED_WORKSPACE = windowKey<string | undefined>("workspace/viewed", undefined);

/** Runs a call, reporting its failure to the frame; resolves to whether it succeeded. */
function useAttempt(): (call: () => Promise<unknown>) => Promise<boolean> {
  const host = useHost();
  return async (call) => {
    try {
      await call();
      return true;
    } catch (error) {
      reportError(host, error);
      return false;
    }
  };
}

export function WorkspaceWidget(_props: WidgetProps): ReactNode {
  const [selected] = useWindowState(SELECTED_THREAD);
  const summary = useThreadSummary(selected);
  const [viewed, setViewed] = useWindowState(VIEWED_WORKSPACE);
  const previous = useRef(selected);
  useEffect(() => {
    if (previous.current !== selected) {
      previous.current = selected;
      setViewed(undefined);
    }
  }, [selected, setViewed]);
  const shown = viewed ?? summary?.workspaceId;
  const [worktreeRepository, setWorktreeRepository] = useState<string | undefined>(undefined);
  return (
    <section
      className="omfp-workspace"
      data-testid="workspaces-panel"
      data-omfp-scope="workspace"
      aria-label={text.label}
    >
      {shown === undefined ? (
        <p className="omfp-muted" data-testid="no-workspace">
          {text.none}
        </p>
      ) : (
        <CurrentWorkspace
          key={shown}
          workspaceId={shown}
          worktreeRepository={worktreeRepository}
          onWorktreeRepository={setWorktreeRepository}
        />
      )}
      <AllWorkspaces onOpen={setViewed} />
      <KnownRepositories canAdd={shown !== undefined} onNewWorktree={setWorktreeRepository} />
    </section>
  );
}

function CurrentWorkspace({
  workspaceId,
  worktreeRepository,
  onWorktreeRepository,
}: {
  workspaceId: string;
  worktreeRepository: string | undefined;
  onWorktreeRepository: (id: string | undefined) => void;
}): ReactNode {
  const { workspace, status } = useWorkspace(workspaceId);
  const threads = useList(useThreads());
  const workspaces = useList(useWorkspaces());
  const call = useCall();
  const attempt = useAttempt();
  if (workspace === undefined) {
    return null;
  }
  const row = workspaceRow(workspace, threads, workspaces, status);
  const add = (source: RepoSource) => attempt(() => call("workspaces.add_repo", { workspaceId, source }));
  return (
    <article className="omfp-workspace__current" data-testid="workspace-current" data-workspace-id={workspaceId}>
      <WorkspaceName workspace={workspace} />
      <div className="omfp-workspace__meta">
        <Badge>{row.owned ? text.owned : text.folder}</Badge>
        {row.shared && (
          <Badge tone="warning" data-testid="shared-warning" title={text.sharedHint}>
            {text.shared(row.threads.length)}
          </Badge>
        )}
        <Button
          size="sm"
          data-testid="refresh-workspace"
          onPress={() => void attempt(() => call("workspaces.rescan", { workspaceId }))}
        >
          {text.rescan}
        </Button>
      </div>
      <code className="omfp-workspace__path" data-testid="workspace-path" title={text.path}>
        {row.path}
      </code>
      <Threads row={row} />
      <Repos row={row} />
      <AddForms add={add} worktreeRepository={worktreeRepository} onWorktreeRepository={onWorktreeRepository} />
    </article>
  );
}

function useList<T>(map: ReadonlyMap<string, T>): readonly T[] {
  return useMemo(() => [...map.values()], [map]);
}

function WorkspaceName({ workspace }: { workspace: Workspace }): ReactNode {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(workspace.name);
  const call = useCall();
  const attempt = useAttempt();
  if (!editing) {
    return (
      <div className="omfp-workspace__title">
        <h2 data-testid="workspace-name">{workspace.name}</h2>
        <Button
          size="sm"
          variant="quiet"
          data-testid="rename-workspace"
          onPress={() => {
            setName(workspace.name);
            setEditing(true);
          }}
        >
          {text.rename}
        </Button>
      </div>
    );
  }
  const save = (event: FormEvent) => {
    event.preventDefault();
    setEditing(false);
    const trimmed = name.trim();
    if (trimmed !== "" && trimmed !== workspace.name) {
      void attempt(() => call("workspaces.update", { workspaceId: workspace.id, name: trimmed }));
    }
  };
  return (
    <form className="omfp-workspace__title" data-testid="workspace-name-form" onSubmit={save}>
      <TextField
        label={text.nameLabel}
        hideLabel
        value={name}
        onChange={setName}
        // The editor opens on request and takes the focus (UI-092)
        autoFocus
        inputProps={{
          "data-testid": "workspace-name-input",
          onKeyDown: (event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              setEditing(false);
            }
          },
        }}
      />
      <Button type="submit" size="sm" variant="primary" data-testid="workspace-name-save">
        {text.save}
      </Button>
    </form>
  );
}

function Threads({ row }: { row: WorkspaceRow }): ReactNode {
  const actions = useThreadActions();
  if (row.threads.length === 0) {
    return null;
  }
  return (
    <section className="omfp-workspace__threads" aria-label={text.threads}>
      <h3>{text.threads}</h3>
      <ul>
        {row.threads.map((thread) => (
          <li key={thread.id}>
            <Button
              size="sm"
              variant="quiet"
              data-testid="workspace-thread"
              data-thread-id={thread.id}
              onPress={() => actions.select(thread.id)}
            >
              {thread.title}
            </Button>
          </li>
        ))}
      </ul>
    </section>
  );
}

function Repos({ row }: { row: WorkspaceRow }): ReactNode {
  const call = useCall();
  const attempt = useAttempt();
  const [confirming, setConfirming] = useState<RepoRow | undefined>(undefined);
  const remove = (name: string) => void attempt(() => call("workspaces.remove_repo", { workspaceId: row.id, name }));
  return (
    <section className="omfp-workspace__repos" aria-label={text.repos}>
      <h3>{text.repos}</h3>
      {row.repos.length === 0 ? (
        <p className="omfp-muted">{text.noRepo}</p>
      ) : (
        <ul>
          {row.repos.map((repo) => (
            <li
              key={repo.name}
              className="omfp-workspace__repo"
              data-testid="workspace-repo"
              data-kind={repo.kind}
              data-name={repo.name}
              data-dirty={repo.dirty === undefined ? undefined : String(repo.dirty)}
            >
              <strong>{repo.label}</strong>
              <span className="omfp-muted" data-testid="repo-origin">
                {repo.origin}
              </span>
              {repo.git === undefined ? (
                <span className="omfp-muted">{text.unknownStatus}</span>
              ) : (
                <span data-testid="git-status">{repo.git}</span>
              )}
              {repo.alsoIn.length > 0 && (
                <Badge tone="warning" data-testid="shared-repo" title={text.alsoInHint}>
                  {text.alsoIn(repo.alsoIn.join(", "))}
                </Badge>
              )}
              <Button
                size="sm"
                variant="quiet"
                data-testid="remove-repo"
                onPress={() => (removalNeedsConfirmation(repo.kind) ? setConfirming(repo) : remove(repo.name))}
              >
                {text.remove}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <ConfirmDialog
        isOpen={confirming !== undefined}
        onOpenChange={(open) => {
          if (!open) {
            setConfirming(undefined);
          }
        }}
        title={text.removeRepoTitle(confirming?.name ?? "")}
        message={text.removeRepoMessage}
        confirmLabel={text.removeNow}
        cancelLabel={text.cancel}
        tone="danger"
        onConfirm={() => {
          if (confirming !== undefined) {
            remove(confirming.name);
          }
        }}
        testId="remove-repo-dialog"
        confirmTestId="confirm-remove-repo"
        cancelTestId="cancel-remove-repo"
      />
    </section>
  );
}

/** Link a folder, link a known repository, new worktree, clone — into the shown workspace. */
function AddForms({
  add,
  worktreeRepository,
  onWorktreeRepository,
}: {
  add: (source: RepoSource) => Promise<boolean>;
  worktreeRepository: string | undefined;
  onWorktreeRepository: (id: string | undefined) => void;
}): ReactNode {
  const repositories = useList(useRepositories());
  return (
    <div className="omfp-workspace__add">
      <OneFieldForm
        testid="link-repo"
        inputTestId="link-repo-path"
        label={text.linkPath}
        placeholder={text.linkPlaceholder}
        submitLabel={text.addLink}
        onSubmit={(path) => add({ kind: "link", path })}
      />
      <KnownRepositoryForm repositories={repositories} add={add} />
      <WorktreeForm
        repositories={repositories}
        repository={worktreeRepository}
        onRepository={onWorktreeRepository}
        add={add}
      />
      <OneFieldForm
        testid="clone-repo"
        inputTestId="clone-repo-url"
        label={text.cloneUrl}
        placeholder={text.clonePlaceholder}
        submitLabel={text.addClone}
        onSubmit={(url) => add({ kind: "clone", url })}
      />
    </div>
  );
}

/** A form with one text field (`<testid>-path`/`-url`…) and a submit button; the field clears on success. */
function OneFieldForm({
  testid,
  label,
  placeholder,
  submitLabel,
  onSubmit,
  inputTestId,
}: {
  testid: string;
  label: string;
  placeholder: string;
  submitLabel: string;
  onSubmit: (value: string) => Promise<boolean>;
  inputTestId: string;
}): ReactNode {
  const [value, setValue] = useState("");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const trimmed = value.trim();
    if (trimmed !== "") {
      void onSubmit(trimmed).then((ok) => ok && setValue(""));
    }
  };
  return (
    <form className="omfp-inline-form" data-testid={testid} onSubmit={submit}>
      <TextField
        label={label}
        hideLabel
        placeholder={placeholder}
        value={value}
        onChange={setValue}
        inputProps={{ "data-testid": inputTestId }}
      />
      <Button type="submit" size="sm" data-testid={`${testid}-submit`}>
        {submitLabel}
      </Button>
    </form>
  );
}

function KnownRepositoryForm({
  repositories,
  add,
}: {
  repositories: readonly Repository[];
  add: (source: RepoSource) => Promise<boolean>;
}): ReactNode {
  const [chosen, setChosen] = useState<string | undefined>(undefined);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const repository = repositories.find((candidate) => candidate.id === chosen);
    if (repository !== undefined) {
      void add({ kind: "link", path: repository.path }).then((ok) => ok && setChosen(undefined));
    }
  };
  return (
    <form className="omfp-inline-form" data-testid="add-known-repo" onSubmit={submit}>
      <Select
        label={text.knownRepository}
        hideLabel
        placeholder={text.knownPlaceholder}
        data-testid="known-repo-choice"
        items={repositories.map((repository) => ({ id: repository.id, label: repository.name }))}
        selectedKey={chosen ?? null}
        onSelectionChange={(key: Key | null) => setChosen(key === null ? undefined : String(key))}
      />
      <Button type="submit" size="sm" isDisabled={chosen === undefined} data-testid="add-known-repo-submit">
        {text.addKnown}
      </Button>
    </form>
  );
}

function WorktreeForm({
  repositories,
  repository,
  onRepository,
  add,
}: {
  repositories: readonly Repository[];
  repository: string | undefined;
  onRepository: (id: string | undefined) => void;
  add: (source: RepoSource) => Promise<boolean>;
}): ReactNode {
  const [branch, setBranch] = useState("");
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (repository === undefined) {
      return;
    }
    const name = branch.trim();
    void add({ kind: "worktree", repo: repository, ...(name === "" ? {} : { branch: name }) }).then((ok) => {
      if (ok) {
        setBranch("");
        onRepository(undefined);
      }
    });
  };
  return (
    <form className="omfp-inline-form" data-testid="create-worktree" onSubmit={submit}>
      <Select
        label={text.worktreeRepo}
        hideLabel
        placeholder={text.knownPlaceholder}
        data-testid="worktree-repository"
        items={repositories.map((repository) => ({ id: repository.id, label: repository.name }))}
        selectedKey={repository ?? null}
        onSelectionChange={(key: Key | null) => onRepository(key === null ? undefined : String(key))}
      />
      <TextField
        label={text.worktreeBranch}
        hideLabel
        placeholder={text.worktreeBranch}
        value={branch}
        onChange={setBranch}
        inputProps={{ "data-testid": "worktree-branch-name" }}
      />
      <Button type="submit" size="sm" isDisabled={repository === undefined} data-testid="create-worktree-submit">
        {text.addWorktree}
      </Button>
    </form>
  );
}

/** Every workspace: threads count, open, remove (with a confirmation). */
function AllWorkspaces({ onOpen }: { onOpen: (id: string) => void }): ReactNode {
  const workspaces = useList(useWorkspaces());
  const threads = useList(useThreads());
  const call = useCall();
  const attempt = useAttempt();
  const [confirming, setConfirming] = useState<{ id: string; name: string } | undefined>(undefined);
  return (
    <section className="omfp-workspace__all" aria-labelledby="omfp-all-workspaces">
      <h3 id="omfp-all-workspaces">{text.allWorkspaces}</h3>
      <ul>
        {workspaceList(workspaces, threads).map((row) => (
          <li key={row.id} className="omfp-workspace__row" data-testid="workspace-row" data-workspace-id={row.id}>
            <strong>{row.name}</strong>
            <span className="omfp-muted" data-testid="workspace-thread-count">
              {text.threadCount(row.threads)}
            </span>
            <code className="omfp-workspace__path">{row.path}</code>
            <Button size="sm" variant="quiet" data-testid="open-workspace" onPress={() => onOpen(row.id)}>
              {text.open}
            </Button>
            <Button size="sm" variant="quiet" data-testid="remove-workspace" onPress={() => setConfirming(row)}>
              {text.remove}
            </Button>
          </li>
        ))}
      </ul>
      <ConfirmDialog
        isOpen={confirming !== undefined}
        onOpenChange={(open) => {
          if (!open) {
            setConfirming(undefined);
          }
        }}
        title={text.removeWorkspaceTitle(confirming?.name ?? "")}
        message={text.removeWorkspaceMessage}
        confirmLabel={text.removeNow}
        cancelLabel={text.cancel}
        tone="danger"
        onConfirm={() => {
          if (confirming !== undefined) {
            const workspaceId = confirming.id;
            void attempt(() => call("workspaces.remove", { workspaceId }));
          }
        }}
        testId="remove-workspace-dialog"
        confirmTestId="confirm-remove-workspace"
        cancelTestId="cancel-remove-workspace"
      />
    </section>
  );
}

/** Known repositories: new worktree of one in this workspace, remove (its workspaces stay), add by path. */
function KnownRepositories({
  canAdd,
  onNewWorktree,
}: {
  canAdd: boolean;
  onNewWorktree: (id: string) => void;
}): ReactNode {
  const repositories = useList(useRepositories());
  const workspaces = useList(useWorkspaces());
  const call = useCall();
  const attempt = useAttempt();
  const [confirming, setConfirming] = useState<RepositoryRow | undefined>(undefined);
  const rows = repositoryRows(repositories, workspaces);
  return (
    <section
      className="omfp-workspace__repositories"
      data-testid="repositories"
      aria-labelledby="omfp-known-repositories"
    >
      <h3 id="omfp-known-repositories">{text.knownRepositories}</h3>
      {rows.length === 0 ? (
        <p className="omfp-muted" data-testid="no-repositories">
          {text.noRepositories}
        </p>
      ) : (
        <ul>
          {rows.map((row) => (
            <li key={row.id} className="omfp-workspace__row" data-testid="repository-row" data-repository-id={row.id}>
              <strong>{row.name}</strong>
              <span className="omfp-muted" data-testid="repository-worktrees">
                {text.worktrees(row.worktrees)}
              </span>
              <code className="omfp-workspace__path">{row.path}</code>
              <span className="omfp-muted" data-testid="repository-base">
                {text.base(row.defaultBaseRef)}
              </span>
              <Button
                size="sm"
                variant="quiet"
                isDisabled={!canAdd}
                data-testid="repository-new-worktree"
                onPress={() => onNewWorktree(row.id)}
              >
                {text.addToWorkspace}
              </Button>
              <Button size="sm" variant="quiet" data-testid="remove-repository" onPress={() => setConfirming(row)}>
                {text.remove}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <OneFieldForm
        testid="add-repository"
        inputTestId="add-repository-path"
        label={text.repositoryPath}
        placeholder={text.repositoryPlaceholder}
        submitLabel={text.addRepository}
        onSubmit={(path) => attempt(() => call("repos.add", { path }))}
      />
      <ConfirmDialog
        isOpen={confirming !== undefined}
        onOpenChange={(open) => {
          if (!open) {
            setConfirming(undefined);
          }
        }}
        title={text.removeRepositoryTitle(confirming?.name ?? "")}
        message={<span data-testid="remove-repository-note">{text.removeRepositoryNote}</span>}
        confirmLabel={text.removeNow}
        cancelLabel={text.cancel}
        tone="danger"
        onConfirm={() => {
          if (confirming !== undefined) {
            const repositoryId = confirming.id;
            void attempt(() => call("repos.remove", { repositoryId }));
          }
        }}
        testId="remove-repository-dialog"
        confirmTestId="confirm-remove-repository"
        cancelTestId="cancel-remove-repository"
      />
    </section>
  );
}
