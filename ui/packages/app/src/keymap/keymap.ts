/**
 * The keymap (UI-095): data, not code. A binding maps a key sequence in a mode to a command id, which runs through the
 * command registry (`sdk/commands.ts`), so bindings can be made configurable later without touching any widget.
 *
 * Keys are written as tokens: a printable key is its character (`j`, `G`, `:`, `/`, `]`), other keys their name
 * (`Space`, `Esc`, `Enter`, `Tab`, `PageDown`, …), prefixed by modifiers in the order `Ctrl-`, `Alt-`, `Meta-` (and
 * `Shift-` only for non-printable keys: the character already says it for printable ones). A sequence is tokens
 * separated by spaces: `g g`, `Space n`.
 */

/** `normal`: focus not in a text field; `insert`: a text field has the focus; `cmdline`: the `:` line is open. */
export type Mode = "normal" | "insert" | "cmdline";

export interface Binding {
  readonly mode: Mode;
  /** Tokens separated by spaces, e.g. `g g`. */
  readonly keys: string;
  readonly command: string;
  /** Arguments passed to the command. */
  readonly args?: readonly string[];
}

/** The defaults: vim-style navigation, `:` command line, `Space` leader (UI-095). */
export const DEFAULT_BINDINGS: readonly Binding[] = [
  { mode: "normal", keys: "j", command: "nav.next" },
  { mode: "normal", keys: "k", command: "nav.prev" },
  { mode: "normal", keys: "g g", command: "nav.top" },
  { mode: "normal", keys: "G", command: "nav.bottom" },
  { mode: "normal", keys: "Ctrl-d", command: "nav.halfDown" },
  { mode: "normal", keys: "Ctrl-u", command: "nav.halfUp" },
  { mode: "normal", keys: "/", command: "nav.search" },
  // On the focused row or workspace section of the left column (UI-020, UI-022).
  { mode: "normal", keys: "d d", command: "nav.delete" },
  { mode: "normal", keys: "z a", command: "nav.toggleSection" },
  // The path of the focused element (a path link, a tool card, a changed file) in a file tab (UI-116).
  { mode: "normal", keys: "g f", command: "file.open" },
  { mode: "normal", keys: "i", command: "composer.focus" },
  { mode: "normal", keys: ":", command: "cmdline.open" },
  { mode: "normal", keys: "Space a", command: "thread.nextAttention" },
  { mode: "normal", keys: "] a", command: "thread.nextAttention" },
  { mode: "normal", keys: "Space n", command: "thread.new" },
  { mode: "normal", keys: "Space w", command: "workspace.new" },
  { mode: "normal", keys: "Space r", command: "thread.rename" },
  { mode: "normal", keys: "Space x", command: "thread.archive" },
  { mode: "normal", keys: "Space u", command: "thread.unarchive" },
  { mode: "normal", keys: "Space d", command: "thread.delete" },
  { mode: "normal", keys: "Space s", command: "thread.stop" },
  { mode: "normal", keys: "Space R", command: "thread.restart" },
  { mode: "normal", keys: "Space :", command: "cmdline.open" },
  { mode: "insert", keys: "Esc", command: "focus.leave" },
  // In the composer: clear the draft, then abort; `u` restores the last cleared draft (UI-063). Elsewhere `Ctrl-c`
  // resolves to no command and copies as usual.
  { mode: "insert", keys: "Ctrl-c", command: "composer.interrupt" },
  { mode: "normal", keys: "Ctrl-c", command: "composer.interrupt" },
  { mode: "normal", keys: "u", command: "composer.restore" },
  // One chord reaches the next waiting thread from anywhere, even while typing (UI-021).
  { mode: "normal", keys: "Alt-a", command: "thread.nextAttention" },
  { mode: "insert", keys: "Alt-a", command: "thread.nextAttention" },
  { mode: "normal", keys: "Alt-n", command: "thread.new" },
  { mode: "insert", keys: "Alt-n", command: "thread.new" },
];

/** How long a started sequence waits for its next key. */
export const SEQUENCE_TIMEOUT_MS = 1000;

/** The parts of a `KeyboardEvent` a token is made of. */
export type KeyInput = Pick<KeyboardEvent, "key" | "ctrlKey" | "altKey" | "metaKey" | "shiftKey">;

const NAMED: Readonly<Record<string, string>> = { " ": "Space", Escape: "Esc", Esc: "Esc" };

/** The token of a key press; `undefined` for a lone modifier. */
export function keyToken(event: KeyInput): string | undefined {
  const { key } = event;
  if (key === "Control" || key === "Alt" || key === "Meta" || key === "Shift" || key === "Dead" || key === "") {
    return undefined;
  }
  const name = NAMED[key] ?? key;
  const printable = [...name].length === 1;
  const prefix = `${event.ctrlKey ? "Ctrl-" : ""}${event.altKey ? "Alt-" : ""}${event.metaKey ? "Meta-" : ""}`;
  // Ctrl/Alt chords on letters are written in lower case (`Ctrl-d`), whatever Shift or Caps Lock did to the key.
  const base = printable && prefix !== "" ? name.toLowerCase() : name;
  return `${prefix}${!printable && event.shiftKey ? "Shift-" : ""}${base}`;
}

/** What one key press did. */
export type Match =
  /** A binding's sequence is complete. */
  | { readonly kind: "command"; readonly binding: Binding }
  /** The keys so far start at least one binding: wait for the next. */
  | { readonly kind: "pending"; readonly keys: string }
  /** No binding: the key is left to the page. */
  | { readonly kind: "none" };

/**
 * Matches key presses against bindings, one sequence at a time. A sequence whose next key does not arrive within
 * `timeoutMs` (by the injected clock) is dropped. A complete binding wins over longer ones sharing its prefix, so
 * defaults never bind both `g` and `g g`.
 */
export class KeymapMatcher {
  readonly #bindings: readonly Binding[];
  readonly #now: () => number;
  readonly #timeoutMs: number;
  #pending: { keys: string[]; mode: Mode; at: number } | undefined;

  constructor(bindings: readonly Binding[], now: () => number, timeoutMs = SEQUENCE_TIMEOUT_MS) {
    this.#bindings = bindings;
    this.#now = now;
    this.#timeoutMs = timeoutMs;
  }

  /** The sequence typed so far (for a hint), if any. */
  get pending(): string | undefined {
    return this.#live()?.keys.join(" ");
  }

  feed(token: string, mode: Mode): Match {
    const pending = this.#live();
    const continued = pending !== undefined && pending.mode === mode ? [...pending.keys, token] : undefined;
    const match = continued === undefined ? undefined : this.#match(continued, mode);
    if (match !== undefined) {
      return match;
    }
    // The key does not continue the sequence: it may start a new one.
    return this.#match([token], mode) ?? this.#none();
  }

  reset(): void {
    this.#pending = undefined;
  }

  #match(keys: string[], mode: Mode): Match | undefined {
    const sequence = keys.join(" ");
    const candidates = this.#bindings.filter((binding) => binding.mode === mode);
    const exact = candidates.find((binding) => binding.keys === sequence);
    if (exact !== undefined) {
      this.#pending = undefined;
      return { kind: "command", binding: exact };
    }
    if (candidates.some((binding) => binding.keys.startsWith(`${sequence} `))) {
      this.#pending = { keys, mode, at: this.#now() };
      return { kind: "pending", keys: sequence };
    }
    return undefined;
  }

  #none(): Match {
    this.#pending = undefined;
    return { kind: "none" };
  }

  #live(): { keys: string[]; mode: Mode } | undefined {
    const pending = this.#pending;
    if (pending !== undefined && this.#now() - pending.at > this.#timeoutMs) {
      this.#pending = undefined;
      return undefined;
    }
    return pending;
  }
}

/** The keys bound to `command` in `mode`, for hints (`Space n`). */
export function keysFor(bindings: readonly Binding[], command: string, mode: Mode = "normal"): string[] {
  return bindings
    .filter((binding) => binding.command === command && binding.mode === mode)
    .map((binding) => binding.keys);
}

/** One `[keymap]` entry of the daemon's settings (UI-097): `command: ""` removes the binding of `keys` in `mode`. */
export interface KeymapOverride {
  readonly mode: Mode;
  readonly keys: string;
  readonly command: string;
}

/** `keys` with single spaces between tokens, as bindings are written. */
export function normalizeKeys(keys: string): string {
  return keys.trim().split(/\s+/).join(" ");
}

/**
 * The defaults with `overrides` applied one by one (UI-097): an entry replaces the binding with the same mode and
 * keys (keeping its place), `""` removes it, any other entry is added after the defaults. Later entries win.
 */
export function mergeBindings(defaults: readonly Binding[], overrides: readonly KeymapOverride[]): Binding[] {
  let bindings: Binding[] = [...defaults];
  for (const override of overrides) {
    const keys = normalizeKeys(override.keys);
    const same = (binding: Binding) => binding.mode === override.mode && binding.keys === keys;
    if (override.command === "") {
      bindings = bindings.filter((binding) => !same(binding));
      continue;
    }
    const replacement: Binding = { mode: override.mode, keys, command: override.command };
    const index = bindings.findIndex(same);
    if (index < 0) {
      bindings.push(replacement);
    } else {
      bindings[index] = replacement;
    }
  }
  return bindings;
}
