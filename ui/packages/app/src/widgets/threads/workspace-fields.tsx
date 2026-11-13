/** The left column's workspace fields: "+ workspace" (UI-023) and a section's inline rename (UI-020). */
import { PathField, TextField } from "@omfp/kit";
import type { Workspace } from "@omfp/protocol";
import { type FormEvent, type KeyboardEvent, type ReactNode, type RefObject, useEffect, useState } from "react";
import { errorMessage, useCall, useReportError, useThreadActions } from "../../sdk/index.ts";
import { usePathSources } from "../../sdk/paths.ts";
import { strings } from "../../strings/index.ts";
import { isWorkspacePath, newWorkspaceParams } from "../../vm/threads.ts";

const text = strings.shell.threads;

/** "+ workspace" (UI-023): one field taking a name (optional) or, when it starts with `/` or `~`, the path of a
 * repository to link (completed as in DMN-021); Enter creates the workspace and opens a draft thread in it. With
 * `focus` it takes the focus, unless the user put it somewhere meanwhile (the `:` line opened while the index loads). */
export function NewWorkspace({
  inputRef,
  focus,
}: {
  inputRef: RefObject<HTMLInputElement | null>;
  focus: boolean;
}): ReactNode {
  const call = useCall();
  const actions = useThreadActions();
  const sources = usePathSources();
  const labels = strings.shell.newWorkspace;
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>(undefined);
  useEffect(() => {
    const active = document.activeElement;
    if (focus && (active === null || active === document.body)) {
      inputRef.current?.focus();
    }
  }, [focus, inputRef]);
  const submit = (typed: string) => {
    if (busy) {
      return;
    }
    setBusy(true);
    setError(undefined);
    call("workspaces.create", newWorkspaceParams(typed)).then(
      (workspace) => {
        setBusy(false);
        setValue("");
        actions.openDraft(workspace.id);
      },
      (failure: unknown) => {
        setBusy(false);
        setError(errorMessage(failure));
      },
    );
  };
  return (
    <div
      className="omfp-threads__new-workspace"
      data-testid="new-workspace"
      data-mode={isWorkspacePath(value) ? "path" : "name"}
    >
      <PathField
        label={labels.label}
        hideLabel
        placeholder={labels.placeholder}
        value={value}
        onChange={setValue}
        onSubmit={submit}
        complete={sources.complete}
        inputRef={inputRef}
        data-testid="new-workspace-input"
      />
      {error !== undefined && (
        <p className="omfp-form-error" role="alert" data-testid="new-workspace-error">
          {error}
        </p>
      )}
    </div>
  );
}

export function RenameWorkspace({ workspace, close }: { workspace: Workspace; close: () => void }): ReactNode {
  const call = useCall();
  const report = useReportError();
  const [name, setName] = useState(workspace.name);
  const save = (event: FormEvent) => {
    event.preventDefault();
    close();
    const trimmed = name.trim();
    if (trimmed !== "" && trimmed !== workspace.name) {
      call("workspaces.update", { workspaceId: workspace.id, name: trimmed }).catch(report);
    }
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  };
  return (
    <form className="omfp-threads__rename" onSubmit={save}>
      <TextField
        label={text.renameLabel}
        hideLabel
        value={name}
        onChange={setName}
        onKeyDown={onKeyDown}
        // Opened from the section's menu to type the new name at once
        autoFocus
        inputProps={{ "data-testid": "workspace-name-input" }}
      />
    </form>
  );
}
