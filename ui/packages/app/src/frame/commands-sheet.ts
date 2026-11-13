/**
 * The content of the phone's commands sheet (UI-462, UI-463): what the `:` line can run with the focus where it was
 * (UI-407), in the which-key panel's groups (UI-326), each command once with the name the line takes, its title and
 * its description; the search keeps those whose names, title or description contain it.
 */

import { commandGroup, KEY_GROUPS, type KeyGroup } from "../keymap/which-key.ts";
import type { Command } from "../sdk/index.ts";
import { available } from "./cmdline.ts";
import { builtinDescription } from "./help.ts";

/** One row of the sheet. */
export interface SheetCommand {
  readonly id: string;
  /** What the `:` line takes: the first alias, else the id. */
  readonly name: string;
  readonly title: string;
  /** The command's description, else the built-in one; `undefined` when it would only repeat the title. */
  readonly description: string | undefined;
  /**
   * It completes a first argument now (`args`, which every `useCommand` command has, gives some): choosing it opens
   * the `:` line on its name, with those completions.
   */
  readonly takesArgs: boolean;
}

export interface SheetGroup {
  readonly group: KeyGroup;
  readonly commands: readonly SheetCommand[];
}

/** The commands available in `scopes` matching `query`, by group in {@link KEY_GROUPS} order, by name in a group. */
export function sheetGroups(commands: readonly Command[], scopes: readonly string[], query: string): SheetGroup[] {
  const needle = query.trim().toLowerCase();
  const rows = available(commands, scopes)
    .filter((command) => needle === "" || haystack(command).includes(needle))
    .map(sheetCommand)
    .sort((a, b) => a.name.localeCompare(b.name));
  return KEY_GROUPS.flatMap((group) => {
    const inGroup = rows.filter((row) => commandGroup(row.id) === group);
    return inGroup.length === 0 ? [] : [{ group, commands: inGroup }];
  });
}

function sheetCommand(command: Command): SheetCommand {
  const description = command.description ?? builtinDescription(command.id);
  return {
    id: command.id,
    name: command.aliases?.[0] ?? command.id,
    title: command.title,
    description: description === command.title ? undefined : description,
    takesArgs: (command.args?.([]) ?? []).length > 0,
  };
}

/** What the search looks in, lower case. */
function haystack(command: Command): string {
  return [command.id, ...(command.aliases ?? []), command.title, command.description ?? builtinDescription(command.id)]
    .join("\n")
    .toLowerCase();
}
