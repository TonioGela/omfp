/** L1: the phone's commands sheet's content (UI-462, UI-463). */

import { describe, expect, test } from "bun:test";
import { sheetGroups } from "../../src/frame/commands-sheet.ts";
import type { Command } from "../../src/sdk/index.ts";

const run = () => {};

const COMMANDS: Command[] = [
  { id: "thread.new", title: "New thread", aliases: ["new"], run },
  { id: "theme.set", title: "Theme", aliases: ["theme"], args: () => ["light", "dark"], run },
  { id: "help.open", title: "Help", aliases: ["help"], run },
  { id: "thread.archive", title: "Archive", aliases: ["archive"], enabled: () => false, run },
  { id: "composer.interrupt", title: "Interrupt", scope: "composer", run },
  // Every `useCommand` command has `args`; without completions it runs at once.
  { id: "demo.ping", title: "Ping", description: "Pings the demo", args: () => [], run },
  { id: "thread.new", title: "New thread (threads)", scope: "threads", run },
];

/** `[group, [id, name, takes arguments]…]` per group. */
function shape(scopes: readonly string[], query = "") {
  return sheetGroups(COMMANDS, scopes, query).map((group) => [
    group.group,
    group.commands.map((command) => [command.id, command.name, command.takesArgs]),
  ]);
}

describe("commands sheet content", () => {
  // spec: UI-462
  test("what the `:` line runs where the focus was, in the which-key groups, each command once by its `:` name", () => {
    expect(shape([])).toEqual([
      ["threads", [["thread.new", "new", false]]],
      ["help", [["help.open", "help", false]]],
      ["commands", [["theme.set", "theme", true]]],
      ["other", [["demo.ping", "demo.ping", false]]],
    ]);
    expect(shape(["composer"])[0]).toEqual([
      "threads",
      [
        ["composer.interrupt", "composer.interrupt", false],
        ["thread.new", "new", false],
      ],
    ]);
  });

  // spec: UI-462
  test("a row carries the command's title and its description, the built-in one when it has none", () => {
    const rows = sheetGroups(COMMANDS, [], "").flatMap((group) => group.commands);
    expect(rows.map((row) => [row.id, row.title, row.description])).toEqual([
      ["thread.new", "New thread", "New thread in the current workspace"],
      ["help.open", "Help", "Show every key and command (this help)"],
      ["theme.set", "Theme", "Colour scheme (system, light, dark)"],
      ["demo.ping", "Ping", "Pings the demo"],
    ]);
  });

  // spec: UI-463
  test("the search keeps the commands whose name, id, title or description contain it, any case", () => {
    expect(shape([], "THEME")).toEqual([["commands", [["theme.set", "theme", true]]]]);
    expect(shape([], "pings")).toEqual([["other", [["demo.ping", "demo.ping", false]]]]);
    expect(shape([], "help.op")).toEqual([["help", [["help.open", "help", false]]]]);
    expect(shape([], "  workspace ")).toEqual([["threads", [["thread.new", "new", false]]]]);
    expect(shape([], "nothing like it")).toEqual([]);
  });
});
