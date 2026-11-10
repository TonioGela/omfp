/** The frame's dialogs: the delete confirmation (UI-026) and the token screen (UI-002). */

import { Button, ConfirmDialog, TextField } from "@omfp/kit";
import { type FormEvent, type ReactNode, useState } from "react";
import { DELETE_THREAD, useThreadActions, useThreadSummary, useWindowState } from "../sdk/index.ts";
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
    />
  );
}

/** Asks for the token printed by `omfp daemon` when none is known or the daemon refused it (UI-002). */
export function TokenScreen({ rejected, onSubmit }: { rejected: boolean; onSubmit: (token: string) => void }) {
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
        {rejected && (
          <p className="omfp-form-error" role="alert" data-testid="token-rejected">
            {text.rejected}
          </p>
        )}
        <Button type="submit" variant="primary" data-testid="token-submit">
          {text.submit}
        </Button>
      </form>
    </main>
  );
}
