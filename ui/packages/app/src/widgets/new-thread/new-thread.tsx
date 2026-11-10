/**
 * The new-thread dialog (UI-023): a workspace — new (optional name, optional existing folder) or existing —, any
 * number of repositories to add right away (link a folder, link a known repository, new worktree, clone a URL; pull
 * requests after S-08) and the first message. No title field: the daemon names the thread (DMN-073). The dialog is
 * modal and traps focus (UI-092); the frame renders it while `NEW_THREAD_DIALOG` is set, so `+ thread`, `Space n` and
 * `:new` all open the same one. The created thread becomes the selected one.
 */

import { Button, Dialog, type Key, Radio, RadioGroup, Select, TextArea, TextField } from "@omfp/kit";
import type { Repository } from "@omfp/protocol";
import { type FormEvent, type ReactNode, useMemo, useState } from "react";
import {
  errorMessage,
  NEW_THREAD_DIALOG,
  reportError,
  type SdkClient,
  type ThreadActions,
  useCall,
  useHost,
  useRepositories,
  useThreadActions,
  useWindowState,
  useWorkspaces,
} from "../../sdk/index.ts";
import { strings } from "../../strings/index.ts";
import {
  EMPTY_FORM,
  emptyChoice,
  type NewThreadForm,
  type NewThreadRequest,
  type RepoChoice,
  type RepoChoiceKind,
  toRequest,
  type WorkspaceChoice,
} from "../../vm/new-thread.ts";

const text = strings.shell.newThread;

/** Creates the thread, adds the repositories to its workspace, then sends the first message; selects the thread as
 * soon as it exists. Rejects only when the thread could not be created. */
export async function createThread(
  call: SdkClient["call"],
  actions: Pick<ThreadActions, "select">,
  request: NewThreadRequest,
  onLaterError: (error: unknown) => void,
): Promise<void> {
  const thread = await call("threads.create", request.create);
  actions.select(thread.id);
  try {
    for (const source of request.repos) {
      await call("workspaces.add_repo", { workspaceId: thread.workspaceId, source });
    }
    if (request.prompt !== undefined) {
      await call("thread.prompt", { threadId: thread.id, text: request.prompt });
    }
  } catch (error) {
    onLaterError(error);
  }
}

/** The dialog while `NEW_THREAD_DIALOG` is set. */
export function NewThreadDialog(): ReactNode {
  const [open, setOpen] = useWindowState(NEW_THREAD_DIALOG);
  return (
    <Dialog isOpen={open} onOpenChange={setOpen} title={text.title} data-testid="new-thread-dialog">
      {open && <NewThreadFormView close={() => setOpen(false)} />}
    </Dialog>
  );
}

function NewThreadFormView({ close }: { close: () => void }): ReactNode {
  const host = useHost();
  const call = useCall();
  const actions = useThreadActions();
  const repositoryMap = useRepositories();
  const repositories = useMemo(() => [...repositoryMap.values()], [repositoryMap]);
  const [form, setForm] = useState<NewThreadForm>(EMPTY_FORM);
  const [error, setError] = useState<string | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const validated = toRequest(form, repositories);
    if (!validated.ok) {
      setError(validated.error);
      return;
    }
    setBusy(true);
    createThread(call, actions, validated.request, (later) => reportError(host, later)).then(
      () => close(),
      (failure: unknown) => {
        setBusy(false);
        setError(errorMessage(failure));
      },
    );
  };
  return (
    <form className="omfp-new-thread" onSubmit={submit}>
      <WorkspaceFields choice={form.workspace} onChange={(workspace) => setForm({ ...form, workspace })} />
      <RepoChoices choices={form.repos} repositories={repositories} onChange={(repos) => setForm({ ...form, repos })} />
      <TextArea
        label={text.prompt}
        rows={3}
        value={form.prompt}
        onChange={(prompt) => setForm({ ...form, prompt })}
        inputProps={{ "data-testid": "new-thread-prompt" }}
      />
      {error !== undefined && (
        <p className="omfp-form-error" role="alert" data-testid="new-thread-error">
          {error}
        </p>
      )}
      <div className="omfp-dialog__actions">
        <Button variant="quiet" data-testid="cancel-thread" onPress={close}>
          {text.cancel}
        </Button>
        <Button type="submit" variant="primary" isDisabled={busy} data-testid="create-thread">
          {text.create}
        </Button>
      </div>
    </form>
  );
}

/** New workspace (name, folder) or an existing one. */
function WorkspaceFields({
  choice,
  onChange,
}: {
  choice: WorkspaceChoice;
  onChange: (choice: WorkspaceChoice) => void;
}): ReactNode {
  const workspaceMap = useWorkspaces();
  const workspaces = useMemo(() => [...workspaceMap.values()], [workspaceMap]);
  return (
    <fieldset className="omfp-new-thread__workspace">
      <RadioGroup
        label={text.workspace}
        orientation="horizontal"
        value={choice.kind}
        data-testid="workspace-mode"
        onChange={(kind) =>
          onChange(kind === "existing" ? { kind, id: workspaces[0]?.id ?? "" } : { kind: "new", name: "", path: "" })
        }
      >
        <Radio value="new" data-testid="workspace-new">
          {text.newWorkspace}
        </Radio>
        <Radio value="existing" data-testid="workspace-existing" isDisabled={workspaces.length === 0}>
          {text.existingWorkspace}
        </Radio>
      </RadioGroup>
      {choice.kind === "new" ? (
        <>
          <TextField
            label={text.workspaceName}
            value={choice.name}
            onChange={(name) => onChange({ ...choice, name })}
            // The dialog opens on request; its first field takes the focus (UI-092)
            autoFocus
            inputProps={{ "data-testid": "new-workspace-name" }}
          />
          <TextField
            label={text.workspacePath}
            placeholder={text.workspacePathPlaceholder}
            value={choice.path}
            onChange={(path) => onChange({ ...choice, path })}
            inputProps={{ "data-testid": "new-workspace-path" }}
          />
        </>
      ) : (
        <Select
          label={text.chooseWorkspace}
          data-testid="workspace-choice"
          items={workspaces.map((workspace) => ({ id: workspace.id, label: workspace.name }))}
          selectedKey={choice.id === "" ? null : choice.id}
          onSelectionChange={(key) => onChange({ kind: "existing", id: key === null ? "" : String(key) })}
        />
      )}
    </fieldset>
  );
}

const ADD_BUTTONS: ReadonlyArray<readonly [RepoChoiceKind, string, string]> = [
  ["link", "add-link", text.addLink],
  ["known", "add-known", text.addKnown],
  ["worktree", "add-worktree", text.addWorktree],
  ["clone", "add-clone", text.addClone],
];

/** The repository lines and the buttons adding one of each source. */
function RepoChoices({
  choices,
  repositories,
  onChange,
}: {
  choices: readonly RepoChoice[];
  repositories: readonly Repository[];
  onChange: (choices: readonly RepoChoice[]) => void;
}): ReactNode {
  const replace = (index: number, choice: RepoChoice) =>
    onChange(choices.map((existing, at) => (at === index ? choice : existing)));
  return (
    <fieldset className="omfp-new-thread__repos">
      <legend>{text.repos}</legend>
      {choices.map((choice, index) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: lines have no identity besides their place in the form
        <div key={index} className="omfp-new-thread__repo" data-testid="repo-choice" data-kind={choice.kind}>
          <ChoiceFields
            line={index + 1}
            choice={choice}
            repositories={repositories}
            onChange={(next) => replace(index, next)}
          />
          <Button
            size="sm"
            variant="quiet"
            aria-label={text.removeRepo(index + 1)}
            data-testid="remove-choice"
            onPress={() => onChange(choices.filter((_, at) => at !== index))}
          >
            ×
          </Button>
        </div>
      ))}
      <div className="omfp-new-thread__add">
        {ADD_BUTTONS.map(([kind, testid, label]) => (
          <Button
            key={kind}
            size="sm"
            data-testid={testid}
            isDisabled={kind === "known" && repositories.length === 0}
            onPress={() => onChange([...choices, emptyChoice(kind)])}
          >
            {label}
          </Button>
        ))}
        <Button size="sm" data-testid="add-pr" isDisabled aria-label={`${text.addPr} — ${text.prAfterS08}`}>
          {text.addPr}
        </Button>
      </div>
    </fieldset>
  );
}

function ChoiceFields({
  line,
  choice,
  repositories,
  onChange,
}: {
  line: number;
  choice: RepoChoice;
  repositories: readonly Repository[];
  onChange: (choice: RepoChoice) => void;
}): ReactNode {
  switch (choice.kind) {
    case "link":
      return (
        <TextField
          label={text.linkPath(line)}
          hideLabel
          placeholder={text.workspacePathPlaceholder}
          value={choice.path}
          onChange={(path) => onChange({ ...choice, path })}
          inputProps={{ "data-testid": "link-path" }}
        />
      );
    case "known":
      return (
        <Select
          label={text.knownRepository(line)}
          hideLabel
          data-testid="known-repository"
          items={repositories.map((repository) => ({ id: repository.id, label: repository.name }))}
          selectedKey={choice.repositoryId === "" ? null : choice.repositoryId}
          onSelectionChange={(key: Key | null) =>
            onChange({ ...choice, repositoryId: key === null ? "" : String(key) })
          }
        />
      );
    case "worktree":
      return (
        <>
          <TextField
            label={text.worktreeRepo(line)}
            hideLabel
            placeholder={text.worktreeRepo(line)}
            value={choice.repo}
            onChange={(repo) => onChange({ ...choice, repo })}
            inputProps={{ "data-testid": "worktree-repo" }}
          />
          <TextField
            label={text.worktreeBranch(line)}
            hideLabel
            placeholder={text.worktreeBranch(line)}
            value={choice.branch}
            onChange={(branch) => onChange({ ...choice, branch })}
            inputProps={{ "data-testid": "worktree-branch" }}
          />
          <TextField
            label={text.worktreeBase(line)}
            hideLabel
            placeholder={text.worktreeBase(line)}
            value={choice.baseRef}
            onChange={(baseRef) => onChange({ ...choice, baseRef })}
            inputProps={{ "data-testid": "worktree-base" }}
          />
        </>
      );
    case "clone":
      return (
        <TextField
          label={text.cloneUrl(line)}
          hideLabel
          placeholder={strings.shell.workspace.clonePlaceholder}
          value={choice.url}
          onChange={(url) => onChange({ ...choice, url })}
          inputProps={{ "data-testid": "clone-url" }}
        />
      );
  }
}
