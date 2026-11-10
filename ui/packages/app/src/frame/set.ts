/**
 * `:set <key>=<value>` (UI-097): one setting of the daemon's `[ui]` or `[keymap]`, sent with `settings.set`. Keys:
 * `density`, `notifications`, `fonts.prose|ui|code`, `keymap.<keys>` (normal mode) and `keymap.<mode>.<keys>`;
 * a keymap value is a command id, `""` removes the binding.
 */

import type { Density, NotificationMode, SetSettingsParams } from "@omfp/protocol";
import { type Mode, normalizeKeys } from "../keymap/keymap.ts";
import { strings } from "../strings/index.ts";

type FontRole = "prose" | "ui" | "code";

export const DENSITIES: readonly Density[] = ["compact", "normal"];
export const NOTIFICATIONS: readonly NotificationMode[] = ["background", "always", "never"];
const FONT_ROLES: readonly FontRole[] = ["prose", "ui", "code"];
const MODES: readonly Mode[] = ["normal", "insert", "cmdline"];

/** Completions of the key part (`:set ` …). */
export const SET_KEYS: readonly string[] = [
  ...DENSITIES.map((density) => `density=${density}`),
  ...NOTIFICATIONS.map((mode) => `notifications=${mode}`),
  ...FONT_ROLES.map((role) => `fonts.${role}=`),
  "keymap.",
];

export type ParsedSet =
  | { readonly ok: true; readonly patch: SetSettingsParams }
  | { readonly ok: false; readonly error: string };

const text = strings.shell.settings;

/** The patch for `line` (everything after `:set`; a font value may contain spaces), or what is wrong with it. */
export function parseSet(line: string): ParsedSet {
  const equals = line.indexOf("=");
  if (equals < 0) {
    return { ok: false, error: text.setUsage };
  }
  const key = line.slice(0, equals).trim();
  const value = unquote(line.slice(equals + 1).trim());
  if (key === "density") {
    const density = DENSITIES.find((candidate) => candidate === value);
    return density === undefined ? invalid(key, value) : { ok: true, patch: { ui: { density } } };
  }
  if (key === "notifications") {
    const notifications = NOTIFICATIONS.find((candidate) => candidate === value);
    return notifications === undefined ? invalid(key, value) : { ok: true, patch: { ui: { notifications } } };
  }
  if (key.startsWith("fonts.")) {
    const role = FONT_ROLES.find((candidate) => `fonts.${candidate}` === key);
    return role === undefined || value === ""
      ? invalid(key, value)
      : { ok: true, patch: { ui: { fonts: { [role]: value } } } };
  }
  if (key.startsWith("keymap.")) {
    return parseKeymap(key.slice("keymap.".length), value);
  }
  return { ok: false, error: text.unknownKey(key) };
}

function parseKeymap(rest: string, command: string): ParsedSet {
  const dot = rest.indexOf(".");
  const named = MODES.find((mode) => dot > 0 && rest.slice(0, dot) === mode);
  const mode = named ?? "normal";
  const keys = normalizeKeys(named === undefined ? rest : rest.slice(dot + 1));
  if (keys === "") {
    return { ok: false, error: text.setUsage };
  }
  return { ok: true, patch: { keymap: [{ mode, keys, command }] } };
}

function invalid(key: string, value: string): ParsedSet {
  return { ok: false, error: text.invalidValue(key, value) };
}

/** `"…"` or `'…'` around a value are dropped, so `:set keymap.g x=""` unbinds `g x`. */
function unquote(value: string): string {
  const quoted = value.length >= 2 && (value[0] === '"' || value[0] === "'") && value.at(-1) === value[0];
  return quoted ? value.slice(1, -1) : value;
}
