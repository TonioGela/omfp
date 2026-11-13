/**
 * The content of the phone's commands sheet (UI-462, UI-463): what the `:` line can run with the focus where it was
 * (UI-407), less what a phone does otherwise, in the which-key panel's groups (UI-326), each command once with its
 * title (without its usage) and the name the line takes; the search also looks in the usage and the description.
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
  /** The command's title without its usage (`Switch view`, not `Switch view (:view <id>)`). */
  readonly title: string;
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

/**
 * Commands a phone does otherwise: `threads.toggle` is the area bar's Threads button, `composer.focus` a tap on the
 * composer. The `navigation` group (the moves of the keyboard focus through lists and messages, the newest notice that
 * the mode line shows as a button) is left out as a whole.
 */
const DONE_OTHERWISE: Readonly<Record<string, true>> = { "threads.toggle": true, "composer.focus": true };

/** A parenthesised usage in a title: `(:view <id>)`, `(opens :view with …)`, `(:open <path>[:<line>])`. */
const USAGE = /\s*\([^()]*(?::\w|<)[^()]*\)/g;

/** The commands available in `scopes` matching `query`, by group in {@link KEY_GROUPS} order, by title in a group. */
export function sheetGroups(commands: readonly Command[], scopes: readonly string[], query: string): SheetGroup[] {
  const needle = query.trim().toLowerCase();
  const rows = available(commands, scopes)
    .filter((command) => DONE_OTHERWISE[command.id] === undefined && commandGroup(command.id) !== "navigation")
    .filter((command) => needle === "" || haystack(command).includes(needle))
    .map(
      (command): SheetCommand => ({
        id: command.id,
        name: command.aliases?.[0] ?? command.id,
        title: command.title.replace(USAGE, ""),
        takesArgs: (command.args?.([]) ?? []).length > 0,
      }),
    )
    .sort((a, b) => a.title.localeCompare(b.title));
  return KEY_GROUPS.flatMap((group) => {
    const inGroup = rows.filter((row) => commandGroup(row.id) === group);
    return inGroup.length === 0 ? [] : [{ group, commands: inGroup }];
  });
}

/** What the search looks in, lower case: the id, the aliases, the whole title and the description. */
function haystack(command: Command): string {
  return [command.id, ...(command.aliases ?? []), command.title, command.description ?? builtinDescription(command.id)]
    .join("\n")
    .toLowerCase();
}
