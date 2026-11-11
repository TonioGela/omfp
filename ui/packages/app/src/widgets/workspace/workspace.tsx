/**
 * `omfp:workspace` (UI-025): the current workspace — the selected thread's, else the draft thread's (UI-023). Header:
 * name (renamed in place), folder path (copyable). Repositories: kind, name, branch, git status live from
 * `workspace/<id>`, origin (link target, worktree's main repository), a badge when the checkout is in another workspace
 * with live threads (DMN-027), remove for every kind (confirmation for anything but a link, DMN-025). One "add
 * repository" control with a Link / Clone switch; paths come with suggestions and completion (DMN-021). The daemon
 * rescans on its own (DMN-023); `:rescan` forces it.
 */

import type { Workspace } from "@omfp/protocol";
import { Badge, Button, ConfirmDialog, IconButton, PathField, Radio, RadioGroup, TextField } from "omfp/kit";
import type { RepoKind } from "omfp/sdk";
import {
  ADD_KINDS,
  type AddKind,
  addRepoSource,
  type RepoRow,
  removalNeedsConfirmation,
  reportError,
  strings,
  useAnnounce,
  useCall,
  useCurrentWorkspaceId,
  useHost,
  usePathSources,
  useThreads,
  useWorkspace,
  useWorkspaces,
  type WidgetProps,
  type WorkspaceRow,
  workspaceRow,
} from "omfp/sdk";
import { type FormEvent, type KeyboardEvent, type ReactNode, useMemo, useState } from "react";

const text = strings.shell.workspace;

/** The glyph of each repository kind (its name is the accessible label). */
const KIND_ICON: Record<RepoKind, string> = { link: "↗", worktree: "⎇", clone: "⤓" };

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
  const shown = useCurrentWorkspaceId();
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
        <CurrentWorkspace key={shown} workspaceId={shown} />
      )}
    </section>
  );
}

function CurrentWorkspace({ workspaceId }: { workspaceId: string }): ReactNode {
  const { workspace, status } = useWorkspace(workspaceId);
  const threads = useList(useThreads());
  const workspaces = useList(useWorkspaces());
  if (workspace === undefined) {
    return null;
  }
  const row = workspaceRow(workspace, threads, workspaces, status);
  return (
    <article className="omfp-workspace__current" data-testid="workspace-current" data-workspace-id={workspaceId}>
      <Header workspace={workspace} row={row} />
      <Repos row={row} />
      <AddRepository workspaceId={workspaceId} />
    </article>
  );
}

function useList<T>(map: ReadonlyMap<string, T>): readonly T[] {
  return useMemo(() => [...map.values()], [map]);
}

function Header({ workspace, row }: { workspace: Workspace; row: WorkspaceRow }): ReactNode {
  const attempt = useAttempt();
  const announce = useAnnounce();
  const copy = () =>
    void attempt(() => navigator.clipboard.writeText(row.path)).then((ok) => ok && announce(text.copied));
  return (
    <header className="omfp-workspace__header">
      <WorkspaceName workspace={workspace} />
      <div className="omfp-workspace__folder">
        <code className="omfp-workspace__path" data-testid="workspace-path" title={text.path}>
          {row.path}
        </code>
        <IconButton label={text.copyPath} icon="⎘" data-testid="copy-workspace-path" onPress={copy} />
      </div>
      {row.shared && (
        <div className="omfp-workspace__meta">
          <Badge tone="warning" data-testid="shared-warning" title={text.sharedHint}>
            {text.shared(row.threadCount)}
          </Badge>
        </div>
      )}
    </header>
  );
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

function Repos({ row }: { row: WorkspaceRow }): ReactNode {
  const call = useCall();
  const attempt = useAttempt();
  const [confirming, setConfirming] = useState<RepoRow | undefined>(undefined);
  const remove = (name: string) => void attempt(() => call("workspaces.remove_repo", { workspaceId: row.id, name }));
  return (
    <section className="omfp-workspace__repos" aria-labelledby="omfp-workspace-repos">
      <h3 id="omfp-workspace-repos">{text.repos}</h3>
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
              <span
                className="omfp-workspace__kind"
                role="img"
                aria-label={text.kinds[repo.kind]}
                title={text.kinds[repo.kind]}
                data-testid="repo-kind"
              >
                {KIND_ICON[repo.kind]}
              </span>
              <strong className="omfp-workspace__repo-name">{repo.name}</strong>
              <span className="omfp-workspace__branch" data-testid="repo-branch">
                {repo.branch}
              </span>
              {repo.git !== undefined && (
                <span className="omfp-muted" data-testid="git-status">
                  {repo.git}
                </span>
              )}
              {repo.alsoIn.length > 0 && (
                <Badge tone="warning" data-testid="shared-repo" title={text.alsoInHint}>
                  {text.alsoIn(repo.alsoIn.join(", "))}
                </Badge>
              )}
              <IconButton
                className="omfp-workspace__remove"
                label={text.removeRepo(repo.name)}
                icon="×"
                data-testid="remove-repo"
                onPress={() => (removalNeedsConfirmation(repo.kind) ? setConfirming(repo) : remove(repo.name))}
              />
              {repo.origin !== undefined && (
                <span className="omfp-workspace__origin omfp-muted" data-testid="repo-origin">
                  {repo.origin}
                </span>
              )}
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

/** One control adds a link or a clone (UI-025): a kind switch, the kind's field, one submit. Enter in a field submits
 * too; the fields clear once the repository is added. */
function AddRepository({ workspaceId }: { workspaceId: string }): ReactNode {
  const call = useCall();
  const attempt = useAttempt();
  const { suggest, complete } = usePathSources();
  const [kind, setKind] = useState<AddKind>("link");
  const [path, setPath] = useState("");
  const [url, setUrl] = useState("");
  const source = addRepoSource(kind, { path, url });
  const submit = (typedPath = path) => {
    const chosen = addRepoSource(kind, { path: typedPath, url });
    if (chosen === undefined) {
      return;
    }
    void attempt(() => call("workspaces.add_repo", { workspaceId, source: chosen })).then((ok) => {
      if (ok) {
        setPath("");
        setUrl("");
      }
    });
  };
  const submitOnEnter = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      submit();
    }
  };
  return (
    <section className="omfp-workspace__add" aria-labelledby="omfp-workspace-add" data-testid="add-repo">
      <h3 id="omfp-workspace-add">{text.addRepository}</h3>
      <RadioGroup
        className="omfp-workspace__kinds"
        label={text.addKind}
        hideLabel
        orientation="horizontal"
        value={kind}
        onChange={(value) => setKind(ADD_KINDS.find((candidate) => candidate === value) ?? "link")}
        data-testid="add-repo-kind"
      >
        <div className="omfp-workspace__kind-options">
          {ADD_KINDS.map((candidate) => (
            <Radio key={candidate} value={candidate} data-value={candidate}>
              {text.kinds[candidate]}
            </Radio>
          ))}
        </div>
      </RadioGroup>
      <div className="omfp-workspace__add-fields">
        {kind === "clone" ? (
          <TextField
            label={text.cloneUrl}
            hideLabel
            placeholder={text.clonePlaceholder}
            value={url}
            onChange={setUrl}
            inputProps={{ "data-testid": "add-repo-url", onKeyDown: submitOnEnter }}
          />
        ) : (
          <PathField
            label={text.linkPath}
            hideLabel
            placeholder={text.linkPlaceholder}
            value={path}
            onChange={setPath}
            onSubmit={submit}
            suggest={suggest}
            complete={complete}
            data-testid="add-repo-path"
          />
        )}
        <Button
          size="sm"
          variant="primary"
          isDisabled={source === undefined}
          data-testid="add-repo-submit"
          onPress={() => submit()}
        >
          {text.add}
        </Button>
      </div>
    </section>
  );
}
