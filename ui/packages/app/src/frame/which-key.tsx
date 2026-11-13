/**
 * The which-key panel (UI-320..UI-326): while a started key sequence waits for its next key, a popover above the mode
 * line lists the keys that complete it — each with the title of the command it runs now (registered by the frame, a
 * widget's `useCommand` or an init file), or the group of the longer bindings it starts — from the window's effective
 * bindings. The key handler decides when it shows (keymap/handler.ts); it never takes the focus.
 */

import { Kbd, WhichKey as Panel, WhichKeyGroup, WhichKeyItem } from "@omfp/kit";
import { type ReactNode, useEffect, useReducer, useSyncExternalStore } from "react";
import { type Binding, keyLabel } from "../keymap/keymap.ts";
import {
  type CommandTitle,
  type PendingKeys,
  type WhichKeyEntry,
  type WhichKeyState,
  whichKeySections,
} from "../keymap/which-key.ts";
import { useAnnounce, useHost } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { builtinDescription } from "./help.ts";

const text = strings.keys.whichKey;

export function WhichKey({ state, bindings }: { state: WhichKeyState; bindings: readonly Binding[] }): ReactNode {
  const pending = useSyncExternalStore(state.subscribe, state.get);
  return pending === undefined ? null : <WhichKeyPanel pending={pending} bindings={bindings} />;
}

function WhichKeyPanel({ pending, bindings }: { pending: PendingKeys; bindings: readonly Binding[] }): ReactNode {
  const { commands } = useHost();
  // Commands registered or removed while the panel shows change their titles at once.
  const [, changed] = useReducer((count: number) => count + 1, 0);
  useEffect(() => commands.subscribe(changed), [commands]);
  const titleOf = (binding: Binding): CommandTitle => {
    const command = commands.resolve(binding.command, pending.scopes);
    return {
      title: command?.title ?? builtinDescription(binding.command) ?? binding.command,
      available: command !== undefined,
    };
  };
  const sections = whichKeySections(bindings, pending, titleOf);
  const keys = keyLabel(pending.keys);
  const count = sections.reduce((total, section) => total + section.entries.length, 0);
  const announce = useAnnounce();
  useEffect(() => {
    announce(text.announce(keys, count));
  }, [announce, keys, count]);
  return (
    <Panel
      keys={keys}
      label={text.label(keys)}
      className="omfp-frame__which-key"
      data-testid="which-key"
      data-keys={pending.keys}
      data-mode={pending.mode}
      footer={
        <>
          <Kbd>{keyLabel("Esc")}</Kbd>
          <span>{text.cancel}</span>
        </>
      }
    >
      {sections.map((section) => (
        <WhichKeyGroup
          key={section.group}
          title={sections.length > 1 ? text.groups[section.group] : undefined}
          data-testid="which-key-group"
          data-group={section.group}
        >
          {section.entries.map((entry) => (
            <Entry key={entry.key} entry={entry} />
          ))}
        </WhichKeyGroup>
      ))}
    </Panel>
  );
}

function Entry({ entry }: { entry: WhichKeyEntry }): ReactNode {
  if (entry.kind === "prefix") {
    return (
      <WhichKeyItem
        chip={keyLabel(entry.key)}
        kind="prefix"
        description={entry.group === undefined ? text.count(entry.count) : text.prefix(text.groups[entry.group])}
        data-testid="which-key-entry"
        data-key={entry.key}
      />
    );
  }
  const args = entry.binding.args ?? [];
  return (
    <WhichKeyItem
      chip={keyLabel(entry.key)}
      description={
        <>
          {entry.title}
          {!entry.available && <span className="omfp-visually-hidden"> {text.unavailable}</span>}
        </>
      }
      args={args.length === 0 ? undefined : args.join(" ")}
      disabled={!entry.available}
      data-testid="which-key-entry"
      data-key={entry.key}
      data-command={entry.binding.command}
    />
  );
}
