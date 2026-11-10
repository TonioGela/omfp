/** L1/L4: the keymap as data, its sequence matcher and the global key handler (UI-095). */

import { afterEach, describe, expect, test } from "bun:test";
import { CMDLINE_ATTRIBUTE, createKeyHandler, modeAt } from "../../src/keymap/handler.ts";
import {
  type Binding,
  DEFAULT_BINDINGS,
  KeymapMatcher,
  keysFor,
  keyToken,
  SEQUENCE_TIMEOUT_MS,
} from "../../src/keymap/keymap.ts";
import { CommandRegistry, SCOPE_ATTRIBUTE } from "../../src/sdk/index.ts";

function key(key: string, modifiers: Partial<Record<"ctrlKey" | "altKey" | "metaKey" | "shiftKey", boolean>> = {}) {
  return { key, ctrlKey: false, altKey: false, metaKey: false, shiftKey: false, ...modifiers };
}

/** A clock the test moves. */
function clock() {
  let now = 0;
  return {
    now: () => now,
    advance: (ms: number) => {
      now += ms;
    },
  };
}

afterEach(() => {
  document.body.innerHTML = "";
});

describe("keymap", () => {
  // spec: UI-095
  test("key presses become tokens with modifiers; printable keys keep their character", () => {
    expect(keyToken(key("j"))).toBe("j");
    expect(keyToken(key("G", { shiftKey: true }))).toBe("G");
    expect(keyToken(key(":", { shiftKey: true }))).toBe(":");
    expect(keyToken(key("d", { ctrlKey: true }))).toBe("Ctrl-d");
    expect(keyToken(key("D", { ctrlKey: true, shiftKey: true }))).toBe("Ctrl-d");
    expect(keyToken(key(" "))).toBe("Space");
    expect(keyToken(key("Escape"))).toBe("Esc");
    expect(keyToken(key("Tab", { shiftKey: true }))).toBe("Shift-Tab");
    expect(keyToken(key("a", { altKey: true }))).toBe("Alt-a");
    expect(keyToken(key("Shift", { shiftKey: true }))).toBeUndefined();
  });

  // spec: UI-095, UI-020, UI-022, UI-023, UI-063
  test("the defaults are vim-style, with the command line and the Space leader", () => {
    const table = (command: string, mode: "normal" | "insert" = "normal") => keysFor(DEFAULT_BINDINGS, command, mode);
    expect(table("nav.next")).toEqual(["j"]);
    expect(table("nav.prev")).toEqual(["k"]);
    expect(table("nav.top")).toEqual(["g g"]);
    expect(table("nav.bottom")).toEqual(["G"]);
    expect(table("nav.halfDown")).toEqual(["Ctrl-d"]);
    expect(table("nav.halfUp")).toEqual(["Ctrl-u"]);
    expect(table("nav.search")).toEqual(["/"]);
    expect(table("composer.focus")).toEqual(["i"]);
    expect(table("cmdline.open")).toEqual([":", "Space :"]);
    expect(table("thread.new")).toEqual(["Space n", "Alt-n"]);
    expect(table("workspace.new")).toEqual(["Space w"]);
    expect(table("nav.delete")).toEqual(["d d"]);
    expect(table("nav.toggleSection")).toEqual(["z a"]);
    expect(table("focus.leave", "insert")).toEqual(["Esc"]);
    expect(table("composer.interrupt", "insert")).toEqual(["Ctrl-c"]);
    expect(table("composer.interrupt")).toEqual(["Ctrl-c"]);
    expect(table("composer.restore")).toEqual(["u"]);
    // No single-key binding while typing.
    expect(DEFAULT_BINDINGS.filter((binding) => binding.mode === "insert").map((binding) => binding.keys)).toEqual([
      "Esc",
      "Ctrl-c",
      "Alt-a",
      "Alt-n",
    ]);
  });

  // spec: UI-095, UI-021
  test("sequences complete, wait, or start over on a key that does not continue them", () => {
    const time = clock();
    const matcher = new KeymapMatcher(DEFAULT_BINDINGS, time.now);
    expect(matcher.feed("g", "normal")).toEqual({ kind: "pending", keys: "g" });
    expect(matcher.pending).toBe("g");
    const top = matcher.feed("g", "normal");
    expect(top.kind === "command" && top.binding.command).toBe("nav.top");
    expect(matcher.pending).toBeUndefined();
    expect(matcher.feed("Space", "normal").kind).toBe("pending");
    const next = matcher.feed("a", "normal");
    expect(next.kind === "command" && next.binding.command).toBe("thread.nextAttention");
    matcher.feed("Space", "normal");
    const restart = matcher.feed("j", "normal");
    expect(restart.kind === "command" && restart.binding.command).toBe("nav.next");
    expect(matcher.feed("q", "normal")).toEqual({ kind: "none" });
    expect(matcher.feed("j", "insert")).toEqual({ kind: "none" });
  });

  // spec: UI-095
  test("a sequence whose next key comes too late is dropped", () => {
    const time = clock();
    const matcher = new KeymapMatcher(DEFAULT_BINDINGS, time.now);
    matcher.feed("g", "normal");
    time.advance(SEQUENCE_TIMEOUT_MS + 1);
    expect(matcher.pending).toBeUndefined();
    expect(matcher.feed("g", "normal")).toEqual({ kind: "pending", keys: "g" });
    time.advance(SEQUENCE_TIMEOUT_MS);
    const top = matcher.feed("g", "normal");
    expect(top.kind === "command" && top.binding.command).toBe("nav.top");
  });

  // spec: UI-095
  test("the focused element decides the mode", () => {
    document.body.innerHTML = `
      <input id="text"><input id="box" type="checkbox"><textarea id="area"></textarea>
      <div id="edit" contenteditable="true"></div><button id="button">b</button>
      <input id="cmd" ${CMDLINE_ATTRIBUTE}>`;
    const mode = (id: string) => modeAt(document.getElementById(id));
    expect([mode("text"), mode("box"), mode("area"), mode("button"), mode("cmd")]).toEqual([
      "insert",
      "normal",
      "insert",
      "normal",
      "cmdline",
    ]);
    expect(modeAt(null)).toBe("normal");
  });
});

describe("global key handler", () => {
  function setup(bindings: readonly Binding[] = DEFAULT_BINDINGS) {
    const commands = new CommandRegistry();
    const ran: string[] = [];
    const register = (id: string, scope?: string) =>
      commands.register({
        id,
        title: id,
        ...(scope === undefined ? {} : { scope }),
        run: (args) => {
          ran.push(`${scope ?? "global"}:${id}${args.length > 0 ? ` ${args.join(" ")}` : ""}`);
        },
      });
    const handler = createKeyHandler({ bindings, commands, now: clock().now });
    const press = (target: Element, name: string, modifiers: Parameters<typeof key>[1] = {}) => {
      if (target instanceof HTMLElement) {
        target.focus();
      }
      const event = new KeyboardEvent("keydown", { ...key(name, modifiers), bubbles: true, cancelable: true });
      handler(event);
      return event.defaultPrevented;
    };
    return { register, ran, press };
  }

  // spec: UI-095
  test("keys run the command of the innermost scope around the focus, else the global one", () => {
    document.body.innerHTML = `<div ${SCOPE_ATTRIBUTE}="threads"><button id="row">t</button></div><p id="out" tabindex="-1"></p>`;
    const { register, ran, press } = setup();
    register("nav.next");
    register("nav.next", "threads");
    const row = document.getElementById("row");
    const out = document.getElementById("out");
    if (row === null || out === null) {
      throw new Error("fixture");
    }
    expect(press(row, "j")).toBe(true);
    expect(press(out, "j")).toBe(true);
    expect(ran).toEqual(["threads:nav.next", "global:nav.next"]);
    expect(press(out, "k")).toBe(false);
  });

  // spec: UI-095, UI-021
  test("single keys do nothing while typing; chords and Esc still work", () => {
    document.body.innerHTML = `<input id="field">`;
    const { register, ran, press } = setup();
    register("nav.next");
    register("thread.nextAttention");
    const field = document.getElementById("field");
    if (field === null) {
      throw new Error("fixture");
    }
    expect(press(field, "j")).toBe(false);
    expect(press(field, "a", { altKey: true })).toBe(true);
    expect(ran).toEqual(["global:thread.nextAttention"]);
    // Without a `focus.leave` command Esc just leaves the field.
    expect(document.activeElement).toBe(field);
    expect(press(field, "Escape")).toBe(true);
    expect(document.activeElement).not.toBe(field);
  });

  // spec: UI-063, UI-095
  test("Ctrl-c runs the composer's command only inside the composer; elsewhere it copies as usual", () => {
    document.body.innerHTML = `<form ${SCOPE_ATTRIBUTE}="composer"><textarea id="draft"></textarea><button id="send">s</button></form><input id="filter"><p id="out" tabindex="-1"></p>`;
    const { register, ran, press } = setup();
    register("composer.interrupt", "composer");
    const element = (id: string) => {
      const found = document.getElementById(id);
      if (found === null) {
        throw new Error("fixture");
      }
      return found;
    };
    expect(press(element("draft"), "c", { ctrlKey: true })).toBe(true);
    expect(press(element("send"), "c", { ctrlKey: true })).toBe(true);
    expect(press(element("filter"), "c", { ctrlKey: true })).toBe(false);
    expect(press(element("out"), "c", { ctrlKey: true })).toBe(false);
    expect(ran).toEqual(["composer:composer.interrupt", "composer:composer.interrupt"]);
  });

  // spec: UI-095, UI-092
  test("no binding applies inside a modal dialog; checkboxes keep their Space", () => {
    document.body.innerHTML = `<div role="dialog"><button id="in">x</button></div><input id="box" type="checkbox">`;
    const { register, ran, press } = setup();
    register("nav.next");
    register("thread.new");
    const inside = document.getElementById("in");
    const box = document.getElementById("box");
    if (inside === null || box === null) {
      throw new Error("fixture");
    }
    expect(press(inside, "j")).toBe(false);
    expect(press(box, " ")).toBe(false);
    expect(press(box, "n")).toBe(false);
    expect(ran).toEqual([]);
  });

  // spec: UI-095
  test("the leader waits for its next key and prevents the page's own Space", () => {
    document.body.innerHTML = `<p id="out" tabindex="-1"></p>`;
    const { register, ran, press } = setup([{ mode: "normal", keys: "Space t", command: "theme.set", args: ["dark"] }]);
    register("theme.set");
    const out = document.getElementById("out");
    if (out === null) {
      throw new Error("fixture");
    }
    expect(press(out, " ")).toBe(true);
    expect(press(out, "t")).toBe(true);
    expect(ran).toEqual(["global:theme.set dark"]);
  });
});
