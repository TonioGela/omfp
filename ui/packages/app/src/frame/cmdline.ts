/**
 * The `:` command line (UI-095), which is also the command palette: `:new`, `:archive`, `:theme dark`, … Its entries
 * are the registered commands (by alias or id); the first word names the command, the rest are its arguments.
 */

import type { Command } from "../sdk/index.ts";

export interface CommandLineInput {
  readonly name: string;
  readonly args: readonly string[];
}

/** The command name and its arguments; `undefined` for an empty line. */
export function parseCommandLine(text: string): CommandLineInput | undefined {
  const [name, ...args] = text.trim().split(/\s+/);
  return name === undefined || name === "" ? undefined : { name, args };
}

/** One completion: `value` replaces the line. */
export interface Completion {
  readonly value: string;
  readonly title: string;
  /** The command's id (for key hints). */
  readonly command: string;
}

/**
 * Completions for `text`: while the first word is typed, the enabled commands global or of one of `scopes` (the ones
 * the line runs in, UI-407) whose alias or id starts with it (each command once, named by its first matching alias);
 * after a space, the command's completions of the argument being typed (given the arguments before it) starting with
 * the last word.
 */
export function completions(text: string, commands: readonly Command[], scopes: readonly string[] = []): Completion[] {
  const enabled = unique(
    commands.filter(
      (command) => (command.scope === undefined || scopes.includes(command.scope)) && (command.enabled?.() ?? true),
    ),
  );
  const leading = text.trimStart();
  const space = leading.search(/\s/);
  if (space < 0) {
    const prefix = leading.toLowerCase();
    return enabled.flatMap((command) => {
      const name = [...(command.aliases ?? []), command.id].find((candidate) =>
        candidate.toLowerCase().startsWith(prefix),
      );
      return name === undefined ? [] : [{ value: name, title: command.title, command: command.id }];
    });
  }
  const name = leading.slice(0, space);
  const command = enabled.find((candidate) => candidate.id === name || (candidate.aliases?.includes(name) ?? false));
  if (command === undefined) {
    return [];
  }
  const words = leading
    .slice(space)
    .trim()
    .split(/\s+/)
    .filter((word) => word !== "");
  const typing = /\s$/.test(leading) ? "" : (words.pop() ?? "");
  const before = [name, ...words].join(" ");
  return (command.args?.(words) ?? [])
    .filter((arg) => arg.startsWith(typing))
    .map((arg) => ({ value: `${before} ${arg}`, title: command.title, command: command.id }));
}

/** The scope of the command `name` (an id or alias) when it has no global registration, so runs only there. */
export function onlyScope(name: string, commands: readonly Command[]): string | undefined {
  const named = commands.filter((command) => command.id === name || (command.aliases?.includes(name) ?? false));
  return named.some((command) => command.scope === undefined) ? undefined : named[0]?.scope;
}

/** One command per id: the global one when several scopes register it. */
export function unique(commands: readonly Command[]): Command[] {
  const byId = new Map<string, Command>();
  for (const command of commands) {
    const known = byId.get(command.id);
    if (known === undefined || (known.scope !== undefined && command.scope === undefined)) {
      byId.set(command.id, command);
    }
  }
  return [...byId.values()];
}
