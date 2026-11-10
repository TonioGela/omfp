/**
 * The one global key handler (UI-095): turns key presses into command runs through {@link KeymapMatcher}, in the mode
 * the focused element puts the window in. It listens on the document in the bubbling phase, so widgets that handle a
 * key themselves (the transcript's PageUp/PageDown/Home/End, React Aria's menus and lists) stop it first; inside a
 * modal dialog no binding applies.
 */

import { type CommandRegistry, scopesAt } from "../sdk/index.ts";
import { type Binding, KeymapMatcher, keyToken, type Mode } from "./keymap.ts";

/** Marks the `:` line's input: the window is in `cmdline` mode while it has the focus. */
export const CMDLINE_ATTRIBUTE = "data-omfp-cmdline";

const NON_TEXT_INPUTS = new Set(["button", "checkbox", "radio", "submit", "reset", "range", "color", "file", "image"]);
const TEXT_ROLES = new Set(["textbox", "searchbox", "combobox", "spinbutton"]);
/** Elements whose own Space behaviour (toggle, select) wins over the leader. */
const SPACE_ROLES = new Set(["checkbox", "radio", "switch", "option", "menuitem", "menuitemcheckbox", "menuitemradio"]);

/** The mode `element` (the focused one) puts the window in. */
export function modeAt(element: Element | null): Mode {
  if (element === null) {
    return "normal";
  }
  if (element.closest(`[${CMDLINE_ATTRIBUTE}]`) !== null) {
    return "cmdline";
  }
  return isTextEntry(element) ? "insert" : "normal";
}

function isTextEntry(element: Element): boolean {
  if (element instanceof HTMLInputElement) {
    return !NON_TEXT_INPUTS.has(element.type);
  }
  if (element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement) {
    return true;
  }
  if (element instanceof HTMLElement && element.isContentEditable) {
    return true;
  }
  return TEXT_ROLES.has(element.getAttribute("role") ?? "");
}

function keepsSpace(element: Element | null): boolean {
  if (element === null) {
    return false;
  }
  if (element instanceof HTMLInputElement && (element.type === "checkbox" || element.type === "radio")) {
    return true;
  }
  return SPACE_ROLES.has(element.getAttribute("role") ?? "");
}

export interface KeymapOptions {
  readonly bindings: readonly Binding[];
  readonly commands: CommandRegistry;
  /** Clock for sequence timeouts (`performance.now` in the browser, a counter in tests). */
  readonly now: () => number;
  /** The focused element; default `document.activeElement`. */
  readonly focused?: () => Element | null;
}

/** `keydown` handler for the document; returns it so the frame can add and remove it. */
export function createKeyHandler(options: KeymapOptions): (event: KeyboardEvent) => void {
  const matcher = new KeymapMatcher(options.bindings, options.now);
  const focused = options.focused ?? (() => document.activeElement);
  return (event) => {
    if (event.defaultPrevented || event.isComposing) {
      return;
    }
    const element = focused();
    const token = keyToken(event);
    const modal = element?.closest('[role="dialog"], [role="alertdialog"]') ?? null;
    if (token === undefined || modal !== null) {
      return;
    }
    if (token === "Space" && matcher.pending === undefined && keepsSpace(element)) {
      return;
    }
    const mode = modeAt(element);
    const match = matcher.feed(token, mode);
    if (match.kind === "pending") {
      event.preventDefault();
      return;
    }
    if (match.kind === "none") {
      return;
    }
    const { command, args = [] } = match.binding;
    const ran = options.commands.run(command, args, scopesAt(element));
    if (!ran && command === "focus.leave" && element instanceof HTMLElement) {
      // No widget decides where focus goes: leaving the field is enough (UI-095).
      element.blur();
      event.preventDefault();
      return;
    }
    if (ran) {
      event.preventDefault();
    }
  };
}
