/**
 * The `:` command line (UI-095), also the command palette: an input with a listbox of completions (ARIA combobox
 * pattern). Tab completes, the arrows move through the options, Enter runs the line (or the highlighted option),
 * Escape closes. Commands run in the scopes around the element that had the focus before the line opened, and the
 * focus goes back there first.
 */

import { Kbd } from "@omfp/kit";
import { type KeyboardEvent, type ReactNode, useEffect, useId, useReducer, useRef, useState } from "react";
import { CMDLINE_ATTRIBUTE } from "../keymap/handler.ts";
import { type Binding, keyLabel, keysFor } from "../keymap/keymap.ts";
import { scopesAt, useCommand, useHost } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { type Completion, completions, parseCommandLine } from "./cmdline.ts";

const text = strings.shell.frame;

export function CommandLine({ bindings }: { bindings: readonly Binding[] }): ReactNode {
  const [open, setOpen] = useState(false);
  const previous = useRef<Element | null>(null);
  useCommand({
    id: "cmdline.open",
    title: strings.shell.commands.cmdline,
    run: () => {
      previous.current = document.activeElement;
      setOpen(true);
    },
  });
  if (!open) {
    return null;
  }
  /** Closes the line; `refocus` puts the focus back where it was (not when the user clicked elsewhere). */
  const close = (refocus: boolean) => {
    setOpen(false);
    const back = previous.current;
    if (refocus && back instanceof HTMLElement && back.isConnected) {
      back.focus();
    }
  };
  return <CommandLineInput bindings={bindings} close={close} scopes={() => scopesAt(previous.current)} />;
}

function CommandLineInput({
  bindings,
  close,
  scopes,
}: {
  bindings: readonly Binding[];
  close: (refocus: boolean) => void;
  scopes: () => string[];
}): ReactNode {
  const { commands } = useHost();
  const [value, setValue] = useState("");
  const [active, setActive] = useState<number | undefined>(undefined);
  const [error, setError] = useState<string | undefined>(undefined);
  const listId = useId();
  // Completions follow commands registered or removed while the line is open.
  const [, changed] = useReducer((count: number) => count + 1, 0);
  useEffect(() => commands.subscribe(changed), [commands]);
  const options = completions(value, commands.list());
  const highlighted = active === undefined ? undefined : options[active];
  const run = (line: string) => {
    const parsed = parseCommandLine(line);
    if (parsed === undefined) {
      close(true);
      return;
    }
    const where = scopes();
    if (commands.resolve(parsed.name, where) === undefined) {
      setError(text.unknownCommand(parsed.name));
      return;
    }
    close(true);
    commands.run(parsed.name, parsed.args, where);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const handled = handleKey(event.key, event.shiftKey);
    if (handled) {
      event.preventDefault();
      event.stopPropagation();
    }
  };
  const handleKey = (key: string, shift: boolean): boolean => {
    switch (key) {
      case "Escape":
        close(true);
        return true;
      case "Enter":
        run(highlighted?.value ?? lineToRun(value, options));
        return true;
      case "Tab": {
        const choice = highlighted ?? options[0];
        if (choice !== undefined && !shift) {
          setValue(`${choice.value} `);
          setActive(undefined);
        }
        return true;
      }
      case "ArrowDown":
      case "ArrowUp":
        setActive(step(active, options.length, key === "ArrowDown" ? 1 : -1));
        return true;
      default:
        return false;
    }
  };
  return (
    <div className="omfp-cmdline" data-testid="command-line">
      <div className="omfp-cmdline__line">
        <span aria-hidden="true">:</span>
        <input
          className="omfp-cmdline__input"
          data-testid="command-line-input"
          {...{ [CMDLINE_ATTRIBUTE]: "" }}
          role="combobox"
          aria-label={text.commandLine}
          aria-expanded={options.length > 0}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={highlighted === undefined ? undefined : `${listId}-${active}`}
          autoComplete="off"
          spellCheck={false}
          // biome-ignore lint/a11y/noAutofocus: the line opens on `:` and takes the focus
          autoFocus
          value={value}
          onChange={(event) => {
            setValue(event.target.value);
            setActive(undefined);
            setError(undefined);
          }}
          onKeyDown={onKeyDown}
          onBlur={() => close(false)}
        />
      </div>
      {error !== undefined && (
        <p className="omfp-cmdline__error" role="alert" data-testid="command-line-error">
          {error}
        </p>
      )}
      <div className="omfp-cmdline__options" role="listbox" id={listId} aria-label={text.commandLine}>
        {options.map((option, index) => (
          <CommandOption
            key={option.value}
            id={`${listId}-${index}`}
            option={option}
            selected={index === active}
            keys={keysFor(bindings, option.command)}
            onChoose={() => run(option.value)}
          />
        ))}
      </div>
    </div>
  );
}

/** The typed line when it is a completion itself, else the first completion it starts (`arch` → `archive`). */
function lineToRun(value: string, options: readonly Completion[]): string {
  const typed = value.trim();
  if (options.some((option) => option.value === typed)) {
    return typed;
  }
  return options.find((option) => option.value.startsWith(typed))?.value ?? typed;
}

function step(active: number | undefined, count: number, delta: number): number | undefined {
  if (count === 0) {
    return undefined;
  }
  if (active === undefined) {
    return delta > 0 ? 0 : count - 1;
  }
  return (active + delta + count) % count;
}

function CommandOption({
  id,
  option,
  selected,
  keys,
  onChoose,
}: {
  id: string;
  option: Completion;
  selected: boolean;
  keys: readonly string[];
  onChoose: () => void;
}): ReactNode {
  return (
    // biome-ignore lint/a11y/useFocusableInteractive: options of a combobox stay unfocusable (aria-activedescendant)
    // biome-ignore lint/a11y/useKeyWithClickEvents: the combobox input handles the keys of its options
    <div
      id={id}
      role="option"
      aria-selected={selected}
      className="omfp-cmdline__option"
      data-testid="command-option"
      data-value={option.value}
      // Keeps the focus in the input, which would otherwise blur and close the line before the click.
      onMouseDown={(event) => event.preventDefault()}
      onClick={onChoose}
    >
      <span className="omfp-cmdline__name">{option.value}</span>
      <span className="omfp-cmdline__title">{option.title}</span>
      {keys.map((key) => (
        <Kbd key={key}>{keyLabel(key)}</Kbd>
      ))}
    </div>
  );
}
