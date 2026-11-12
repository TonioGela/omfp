/** The frame's dialogs: the thread and workspace delete confirmations (UI-026, UI-020) and the token screen (UI-002). */

import { Button, Checkbox, ConfirmDialog, Dialog, TextField } from "@omfp/kit";
import type { ThreadSummary, Workspace } from "@omfp/protocol";
import { type FormEvent, type ReactNode, useMemo, useState } from "react";
import { keyLabel } from "../keymap/keymap.ts";
import {
  DELETE_THREAD,
  DELETE_WORKSPACE,
  removeWorkspace,
  useHost,
  useThreadActions,
  useThreadSummary,
  useThreads,
  useWindowState,
  useWorkspaces,
} from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { displayTitle } from "../vm/threads.ts";

/** Deleting removes the thread's history, never its workspace; Cancel has the focus when it opens. */
export function DeleteThreadDialog(): ReactNode {
  const [threadId, setThreadId] = useWindowState(DELETE_THREAD);
  const summary = useThreadSummary(threadId);
  const actions = useThreadActions();
  const text = strings.shell.deleteThread;
  return (
    <ConfirmDialog
      isOpen={threadId !== undefined}
      onOpenChange={(open) => {
        if (!open) {
          setThreadId(undefined);
        }
      }}
      title={text.title(summary === undefined ? (threadId ?? "") : displayTitle(summary))}
      message={<span data-testid="delete-thread-message">{text.message}</span>}
      confirmLabel={text.confirm}
      cancelLabel={text.cancel}
      tone="danger"
      onConfirm={() => {
        if (threadId !== undefined) {
          void actions.remove(threadId);
        }
      }}
      testId="delete-thread-dialog"
      confirmTestId="confirm-delete-thread"
      cancelTestId="cancel-delete-thread"
      cancelKbd={keyLabel("Escape")}
    />
  );
}

/**
 * Deleting a workspace (UI-020, DMN-028): the daemon refuses while it has threads, so the dialog lists them and
 * deletes them first once "delete the threads too" is ticked; the folder is deleted, linked projects are kept. Cancel
 * has the focus when it opens.
 */
export function DeleteWorkspaceDialog(): ReactNode {
  const [workspaceId, setWorkspaceId] = useWindowState(DELETE_WORKSPACE);
  const workspace = useWorkspaces().get(workspaceId ?? "");
  const threadMap = useThreads();
  const threads = useMemo(
    () => [...threadMap.values()].filter((thread) => thread.workspaceId === workspaceId),
    [threadMap, workspaceId],
  );
  return (
    <Dialog
      isOpen={workspace !== undefined}
      onOpenChange={(open) => {
        if (!open) {
          setWorkspaceId(undefined);
        }
      }}
      title={strings.shell.deleteWorkspace.title(workspace?.name ?? "")}
      role="alertdialog"
      data-testid="delete-workspace-dialog"
    >
      {({ close }) =>
        workspace === undefined ? null : (
          <DeleteWorkspaceForm key={workspace.id} workspace={workspace} threads={threads} close={close} />
        )
      }
    </Dialog>
  );
}

function DeleteWorkspaceForm({
  workspace,
  threads,
  close,
}: {
  workspace: Workspace;
  threads: readonly ThreadSummary[];
  close: () => void;
}): ReactNode {
  const host = useHost();
  const [deleteThreads, setDeleteThreads] = useState(false);
  const text = strings.shell.deleteWorkspace;
  const blocked = threads.length > 0 && !deleteThreads;
  return (
    <>
      <div className="omfp-dialog__message" data-testid="delete-workspace-message">
        <p>{text.message(workspace.path)}</p>
        {threads.length > 0 && (
          <>
            <p>{text.threads(threads.length)}</p>
            <ul data-testid="delete-workspace-thread-list">
              {threads.map((thread) => (
                <li key={thread.id} data-thread-id={thread.id}>
                  {displayTitle(thread)}
                </li>
              ))}
            </ul>
            <Checkbox isSelected={deleteThreads} onChange={setDeleteThreads} data-testid="delete-workspace-threads">
              {text.deleteThreads}
            </Checkbox>
          </>
        )}
      </div>
      <div className="omfp-dialog__actions">
        <Button
          variant="quiet"
          onPress={close}
          autoFocus
          kbd={keyLabel("Escape")}
          data-testid="cancel-delete-workspace"
        >
          {text.cancel}
        </Button>
        <Button
          variant="danger"
          isDisabled={blocked}
          data-testid="confirm-delete-workspace"
          onPress={() => {
            close();
            void removeWorkspace(host, workspace.id, deleteThreads ? threads.map((thread) => thread.id) : []);
          }}
        >
          {text.confirm}
        </Button>
      </div>
    </>
  );
}

/**
 * Asks for the token printed by `omfp daemon` when none is known or the daemon refused it (UI-002); `rejected` says why
 * the last token or link did not connect (UI-255).
 */
export function TokenScreen({
  rejected,
  onSubmit,
}: {
  rejected: string | undefined;
  onSubmit: (token: string) => void;
}) {
  const [token, setToken] = useState("");
  const text = strings.shell.token;
  const submit = (event: FormEvent) => {
    event.preventDefault();
    const value = token.trim();
    if (value !== "") {
      onSubmit(value);
    }
  };
  return (
    <main className="omfp-token" data-testid="token-screen">
      <form className="omfp-token__form" aria-labelledby="omfp-token-title" onSubmit={submit}>
        <h1 id="omfp-token-title">{text.title}</h1>
        <p>{text.help}</p>
        <TextField
          label={text.label}
          type="password"
          autoComplete="off"
          value={token}
          onChange={setToken}
          // The token is the only thing the page asks for
          autoFocus
          inputProps={{ "data-testid": "token-input" }}
        />
        {rejected !== undefined && (
          <p className="omfp-form-error" role="alert" data-testid="token-rejected">
            {rejected}
          </p>
        )}
        <Button type="submit" variant="primary" data-testid="token-submit">
          {text.submit}
        </Button>
      </form>
    </main>
  );
}
