/**
 * The phone's commands sheet (UI-460..UI-464): the Commands button of the area bar opens it over the whole frame — a
 * search field, then what the `:` line can run with the focus where it was (the button leaves it there), grouped
 * (commands-sheet.ts). Choosing a command closes the sheet, gives the focus back and runs the command in those scopes;
 * one completing arguments opens the `:` line on its name instead, with their completions. Escape, the close button
 * and the browser's back (a phone's back gesture) close it.
 */

import { Icon, Sheet, SheetRow, SheetSection, TextField } from "@omfp/kit";
import { type ReactNode, useEffect, useReducer, useRef, useState } from "react";
import { scopesAt, useHost } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { type SheetCommand, sheetGroups } from "./commands-sheet.ts";

const text = strings.shell.commandsSheet;

/** The key of the history entry the open sheet adds. */
const BACK_ENTRY = "omfpCommandsSheet";

/** The sheet, mounted while it is open; `onClose` unmounts it. */
export function CommandsSheet({ onClose }: { onClose: () => void }): ReactNode {
  const { commands } = useHost();
  // Read as it opens, before the dialog takes the focus.
  const [origin] = useState(() => document.activeElement);
  const [scopes] = useState(() => scopesAt(origin));
  const chosen = useRef<(() => void) | undefined>(undefined);
  // Commands registered or removed while it is open show at once.
  const [, changed] = useReducer((count: number) => count + 1, 0);
  useEffect(() => commands.subscribe(changed), [commands]);
  const [query, setQuery] = useState("");
  useBackCloses(onClose);
  // Once it is gone: the focus back where it was, then the chosen command, as the `:` line runs one.
  useEffect(
    () => () => {
      if (origin instanceof HTMLElement && origin.isConnected) {
        origin.focus();
      }
      chosen.current?.();
    },
    [origin],
  );
  const choose = (command: SheetCommand) => {
    chosen.current = command.takesArgs
      ? () => commands.run("cmdline.open", [command.name])
      : () => commands.run(command.id, [], scopes);
    onClose();
  };
  const groups = sheetGroups(commands.list(), scopes, query);
  return (
    <Sheet
      title={text.title}
      closeLabel={text.close}
      onClose={onClose}
      data-testid="commands-sheet"
      search={
        <TextField
          label={text.search}
          hideLabel
          icon={<Icon name="search" />}
          placeholder={text.placeholder}
          value={query}
          onChange={setQuery}
          inputProps={{ "data-testid": "commands-search", autoComplete: "off", spellCheck: false }}
        />
      }
    >
      {groups.length === 0 ? (
        <p className="omfp-sheet__empty" data-testid="commands-empty">
          {text.noMatch}
        </p>
      ) : (
        groups.map(({ group, commands: rows }) => (
          <SheetSection key={group} title={text.groups[group]} data-testid="commands-group" data-group={group}>
            {rows.map((command) => (
              <SheetRow
                key={command.id}
                name={`:${command.name}`}
                label={command.title}
                more={command.takesArgs}
                data-testid="commands-row"
                data-command={command.id}
                onPress={() => choose(command)}
              />
            ))}
          </SheetSection>
        ))
      )}
    </Sheet>
  );
}

/**
 * While mounted, an entry of the window's history whose back calls `close` (a phone's back gesture closes the sheet,
 * not the app); closed another way, it takes its entry back.
 */
function useBackCloses(close: () => void): void {
  const latest = useRef(close);
  latest.current = close;
  useEffect(() => {
    let popped = false;
    window.history.pushState({ [BACK_ENTRY]: true }, "");
    const onPop = () => {
      popped = true;
      latest.current();
    };
    window.addEventListener("popstate", onPop);
    return () => {
      window.removeEventListener("popstate", onPop);
      const state: unknown = window.history.state;
      if (!popped && typeof state === "object" && state !== null && BACK_ENTRY in state) {
        window.history.back();
      }
    };
  }, []);
}
