/**
 * The file picker (UI-117): when a path that does not exist names several files of the workspace
 * (`workspaces.find_file`, DMN-092), the frame offers them in the style of the `:` line — an input filtering the
 * candidates, the arrows moving through them (the best one highlighted), Enter opening the highlighted one in place
 * of the missing path's tab, Escape (or leaving the line) cancelling the opening.
 */

import { CommandItem, CommandMenu, CommandLine as Prompt } from "@omfp/kit";
import type { FileCandidate } from "@omfp/protocol";
import { type KeyboardEvent, type ReactNode, useId, useState } from "react";
import { CMDLINE_ATTRIBUTE } from "../keymap/handler.ts";
import { cancelFilePicker, chooseFile, FILE_PICKER, type FilePicker, useHost, useWindowState } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { MenuKeys } from "./command-line.tsx";

const text = strings.shell.files;

export function FilePickerLine(): ReactNode {
  const [picker] = useWindowState(FILE_PICKER);
  // A new picker starts afresh (its filter and highlight).
  return picker === undefined ? null : <PickerInput key={`${picker.workspaceId}:${picker.path}`} picker={picker} />;
}

function PickerInput({ picker }: { picker: FilePicker }): ReactNode {
  const host = useHost();
  const [value, setValue] = useState("");
  const [active, setActive] = useState(0);
  const listId = useId();
  const typed = value.trim().toLowerCase();
  const shown = picker.candidates.filter((candidate) => candidate.path.toLowerCase().includes(typed));
  const highlighted = shown[Math.min(active, shown.length - 1)];
  const choose = (candidate: FileCandidate | undefined) => {
    if (candidate !== undefined) {
      chooseFile(host, picker.workspaceId, picker.path, candidate.path, picker.options);
    }
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    switch (event.key) {
      case "Escape":
        cancelFilePicker(host);
        break;
      case "Enter":
        choose(highlighted);
        break;
      case "ArrowDown":
      case "ArrowUp":
        if (shown.length > 0) {
          const at = Math.min(active, shown.length - 1);
          setActive((at + (event.key === "ArrowDown" ? 1 : -1) + shown.length) % shown.length);
        }
        break;
      default:
        return;
    }
    event.preventDefault();
    event.stopPropagation();
  };
  return (
    <div className="omfp-cmdline omfp-file-picker" data-testid="file-picker">
      <CommandMenu
        className="omfp-cmdline__menu"
        title={strings.shell.frame.filesMenu}
        hidden={shown.length === 0}
        listProps={{ id: listId, "aria-label": text.picker(picker.path) }}
        footer={<MenuKeys />}
      >
        {shown.map((candidate) => (
          <CommandItem
            key={candidate.path}
            id={`${listId}-${candidate.path}`}
            command={candidate.path}
            selected={candidate === highlighted}
            description={[text[candidate.match], ...(candidate.mentioned ? [text.mentioned] : [])].join(", ")}
            data-testid="file-picker-option"
            data-path={candidate.path}
            // Keeps the focus in the input, which would otherwise blur and cancel the picker before the click.
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => choose(candidate)}
          />
        ))}
      </CommandMenu>
      <Prompt>
        <span className="omfp-file-picker__prompt" aria-hidden="true">
          {text.pickerPrompt} {picker.path}
        </span>
        <input
          className="omfp-cmd__input"
          data-testid="file-picker-input"
          {...{ [CMDLINE_ATTRIBUTE]: "" }}
          role="combobox"
          aria-label={text.picker(picker.path)}
          aria-expanded={shown.length > 0}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={highlighted === undefined ? undefined : `${listId}-${highlighted.path}`}
          autoComplete="off"
          spellCheck={false}
          // biome-ignore lint/a11y/noAutofocus: the picker opens for the path the user just opened and takes the focus
          autoFocus
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
          onBlur={() => cancelFilePicker(host)}
        />
      </Prompt>
    </div>
  );
}
