/**
 * What `:describe-key` and `:describe-command` show (UI-264, UI-265): Emacs-style self-documentation of
 * the effective keymap and the command registry. A key sequence shows each binding of exactly those keys (every mode):
 * the command, its title and description, its arguments, which layer bound it (default, `[keymap]`, the workspace's
 * keymap, an init file) and who registered the command. A command shows its names, scope, who registered it and every
 * binding that runs it.
 */

import { type Binding, keyLabel } from "../keymap/keymap.ts";
import type { Command } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { builtinDescription } from "./help.ts";

const text = strings.init.describe;
const fields = text.fields;

/** One line of a description: a label, then key chips and/or a value. */
export interface DescribeField {
  readonly label: string;
  readonly value: string;
  readonly keys?: readonly string[];
}

export interface DescribeSection {
  readonly id: string;
  readonly title: string;
  readonly fields: readonly DescribeField[];
}

function matches(command: Command, name: string): boolean {
  return command.id === name || (command.aliases?.includes(name) ?? false);
}

/** The lines about `command`, or about an unregistered `id` (its built-in description when it has one). */
function commandFields(id: string, command: Command | undefined): DescribeField[] {
  if (command === undefined) {
    return [
      { label: fields.command, value: id },
      { label: fields.title, value: builtinDescription(id) ?? text.notRegistered },
    ];
  }
  return [
    { label: fields.command, value: command.id },
    { label: fields.title, value: command.title },
    ...(command.description === undefined ? [] : [{ label: fields.description, value: command.description }]),
    { label: fields.defined, value: command.init === undefined ? text.origins.window : text.origins[command.init] },
  ];
}

/** `:describe-key` for `keys` (UI-264): one section per binding of exactly these keys, in binding order. */
export function describeKey(
  keys: string,
  bindings: readonly Binding[],
  commands: readonly Command[],
): DescribeSection[] {
  const found = bindings.filter((binding) => binding.keys === keys);
  if (found.length === 0) {
    return [
      { id: "unbound", title: keyLabel(keys), fields: [{ label: fields.keys, value: text.unbound(keyLabel(keys)) }] },
    ];
  }
  return found.map((binding) => {
    const command =
      commands.find((candidate) => candidate.scope === undefined && matches(candidate, binding.command)) ??
      commands.find((candidate) => matches(candidate, binding.command));
    return {
      id: `${binding.mode}:${binding.keys}`,
      title: text.mode(binding.mode),
      fields: [
        { label: fields.keys, value: "", keys: [keyLabel(binding.keys)] },
        ...commandFields(binding.command, command),
        ...(binding.args === undefined ? [] : [{ label: fields.args, value: binding.args.join(" ") }]),
        { label: fields.bound, value: text.sources[binding.source ?? "default"] },
      ],
    };
  });
}

/**
 * `:describe-command <name>` (UI-265): one section per registration answering to `name` (an id or an alias, each scope
 * its own), with every binding that runs it; a single section saying so when nothing answers.
 */
export function describeCommand(
  name: string,
  bindings: readonly Binding[],
  commands: readonly Command[],
): DescribeSection[] {
  if (name === "") {
    return [{ id: "unknown", title: text.title, fields: [{ label: fields.command, value: text.missingName }] }];
  }
  const found = commands.filter((command) => matches(command, name));
  if (found.length === 0) {
    return [{ id: "unknown", title: name, fields: [{ label: fields.command, value: text.unknown(name) }] }];
  }
  return found.map((command) => {
    const keys = bindings.filter((binding) => binding.command === command.id);
    const names = [command.id, ...(command.aliases ?? [])].map((alias) => `:${alias}`);
    return {
      id: `${command.scope ?? ""}:${command.id}`,
      title: command.id,
      fields: [
        ...commandFields(command.id, command),
        { label: fields.names, value: names.join(" ") },
        { label: fields.scope, value: command.scope ?? text.global },
        ...(keys.length === 0
          ? [{ label: fields.bindings, value: text.noBinding }]
          : keys.map((binding) => ({
              label: text.mode(binding.mode),
              keys: [keyLabel(binding.keys)],
              value: `${binding.args === undefined ? "" : `${binding.args.join(" ")} · `}${text.sources[binding.source ?? "default"]}`,
            }))),
      ],
    };
  });
}
