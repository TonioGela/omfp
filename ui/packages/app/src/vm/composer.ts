/**
 * The composer (UI-060..064): send mode while busy, `/` command completion, key bindings, image attachments, drafts
 * restored after an abort or a failed send, and `Ctrl-C` clearing the draft, then aborting (UI-063).
 */

import type { AvailableSlashCommand, ImageContent, ThreadState } from "@omfp/core";
import type { PromptBehavior } from "@omfp/protocol";
import type { Draft } from "../sdk/index.ts";
import { isObject } from "../sdk/json.ts";

/** Most suggestions shown at once. */
const MAX_SUGGESTIONS = 8;

/** Nothing to send. */
export function isBlank(draft: Draft): boolean {
  return draft.text.trim() === "" && draft.images.length === 0;
}

/** The agent is working: messages sent now are queued (UI-061). */
export function isBusy(state: ThreadState): boolean {
  return !state.settled;
}

/** Esc aborts while the agent works or messages wait in the queue (UI-063); otherwise it leaves the composer. */
export function canAbort(state: ThreadState): boolean {
  return isBusy(state) || state.queue.steering.length > 0 || state.queue.follow_up.length > 0;
}

/** What the send action does: a new turn when idle; while busy queued until the turn ends, or steered into it. */
export type SendMode = "prompt" | "follow-up" | "steer";

/** Busy threads queue as follow-up unless the steer modifier (Shift, Ctrl or ⌘ with Enter) is held. */
export function sendMode(busy: boolean, steerModifier: boolean): SendMode {
  if (!busy) {
    return "prompt";
  }
  return steerModifier ? "steer" : "follow-up";
}

/** The `behavior` of `thread.prompt`. */
export function behavior(mode: SendMode): PromptBehavior | undefined {
  switch (mode) {
    case "prompt":
      return undefined;
    case "follow-up":
      return "followUp";
    case "steer":
      return "steer";
  }
}

/** The other busy mode, offered as a second button. */
export function alternative(mode: SendMode): SendMode | undefined {
  switch (mode) {
    case "prompt":
      return undefined;
    case "follow-up":
      return "steer";
    case "steer":
      return "follow-up";
  }
}

/** Modifier keys held with a key. */
export interface Modifiers {
  readonly shift: boolean;
  readonly alt: boolean;
  readonly ctrl: boolean;
  readonly meta: boolean;
}

/**
 * The modifier that turns a busy send into a steer: Shift (Ctrl and ⌘ too). Alt with Enter is a new line, also on
 * macOS, where Option+Enter arrives as `Enter` with `altKey`.
 */
export function steerModifier(modifiers: Modifiers): boolean {
  return !modifiers.alt && (modifiers.shift || modifiers.ctrl || modifiers.meta);
}

/** What a key press in the composer does. */
export type KeyAction =
  /** Let the text area handle it (typing). */
  | { readonly type: "none" }
  /** Alt+Enter: insert a line break at the caret. */
  | { readonly type: "newline" }
  /** An auto-repeated Enter: swallowed, so holding the key sends one message, not one per repeat. */
  | { readonly type: "ignore" }
  | { readonly type: "send"; readonly mode: SendMode }
  /** Esc while busy or queued: `thread.abort` (UI-063). */
  | { readonly type: "abort" }
  /** Esc otherwise: leave the composer (`focus.leave`, UI-095). */
  | { readonly type: "leave" }
  /** Tab with suggestions: complete the first one. */
  | { readonly type: "complete" };

/** What the composer knows when a key is pressed. */
export interface KeyContext {
  readonly busy: boolean;
  /** Per {@link canAbort}. */
  readonly abortable: boolean;
  readonly hasSuggestions: boolean;
  /** The key is auto-repeating (`KeyboardEvent.repeat`). */
  readonly repeat?: boolean;
}

/** The composer's key bindings. */
export function keyAction(key: string, modifiers: Modifiers, context: KeyContext): KeyAction {
  switch (key) {
    case "Enter":
      if (modifiers.alt) {
        return { type: "newline" };
      }
      return context.repeat === true
        ? { type: "ignore" }
        : { type: "send", mode: sendMode(context.busy, steerModifier(modifiers)) };
    case "Escape":
      return context.abortable ? { type: "abort" } : { type: "leave" };
    case "Tab":
      return context.hasSuggestions && !modifiers.shift ? { type: "complete" } : { type: "none" };
    default:
      return { type: "none" };
  }
}

/** One `/` completion. */
export interface Suggestion {
  readonly name: string;
  readonly description: string | undefined;
  /** The usage hint of the command's argument. */
  readonly hint: string | undefined;
}

/**
 * Commands matching the draft while it is a bare `/prefix` (no space yet): names first, then aliases, in the order of
 * `thread.commands` (kept fresh by `available_commands_update`, UI-060).
 */
export function slashSuggestions(draft: string, commands: readonly AvailableSlashCommand[]): Suggestion[] {
  if (!draft.startsWith("/")) {
    return [];
  }
  const prefix = draft.slice(1);
  if (/\s/.test(prefix)) {
    return [];
  }
  const byName = commands.filter((command) => command.name.startsWith(prefix));
  const byAlias = commands.filter(
    (command) => !command.name.startsWith(prefix) && (command.aliases ?? []).some((alias) => alias.startsWith(prefix)),
  );
  return [...byName, ...byAlias].slice(0, MAX_SUGGESTIONS).map((command) => ({
    name: command.name,
    description: command.description,
    hint: command.input?.hint,
  }));
}

/** The draft text after picking `suggestion`. */
export function complete(suggestion: Suggestion): string {
  return `/${suggestion.name} `;
}

/** The commands of a `thread.commands` result (`{ commands }`), for a thread whose state has none yet. */
export function commandsOf(result: unknown): AvailableSlashCommand[] {
  if (!isObject(result) || !Array.isArray(result.commands)) {
    return [];
  }
  return result.commands.filter(
    (command): command is AvailableSlashCommand => isObject(command) && typeof command.name === "string",
  );
}

/**
 * The draft after an abort: the restored steering and follow-up texts, oldest first, then what was being typed, one
 * blank line apart (UI-063).
 */
export function restoreDraft(draft: string, steering: readonly string[], followUp: readonly string[]): string {
  return [...steering, ...followUp, draft]
    .map((text) => text.trimEnd())
    .filter((text) => text.trim() !== "")
    .join("\n\n");
}

/**
 * The draft after a prompt failed to send: the failed text, then what was typed since, one blank line apart; the
 * failed images, then the images attached since. Nothing the user wrote is lost (UI-064).
 */
export function restoreFailedPrompt(draft: Draft, text: string, images: readonly ImageContent[]): Draft {
  return { text: restoreDraft(draft.text, [text], []), images: [...images, ...draft.images] };
}

/** A second `Ctrl-C` within this many milliseconds of the one that cleared the draft aborts (UI-063). */
export const INTERRUPT_WINDOW_MS = 1000;

/** What `Ctrl-C` in the composer does (UI-063). */
export type Interrupt = "clear" | "abort";

/**
 * `Ctrl-C` at `now` (UI-063): clears a non-empty draft; aborts on an empty one, or when the previous `Ctrl-C`
 * (`lastPress`) was at most {@link INTERRUPT_WINDOW_MS} ago.
 */
export function interruptAction(draft: Draft, now: number, lastPress: number | undefined): Interrupt {
  if (lastPress !== undefined && now - lastPress <= INTERRUPT_WINDOW_MS) {
    return "abort";
  }
  return draft.text === "" && draft.images.length === 0 ? "abort" : "clear";
}

/** The draft after `u` restored the draft `Ctrl-C` cleared: its text and images first, then what was typed since
 * (UI-063). */
export function restoreCleared(draft: Draft, cleared: Draft): Draft {
  return restoreFailedPrompt(draft, cleared.text, cleared.images);
}

/** The texts of a `thread.abort` result (`{ steering: [{ text }], followUp: [{ text }] }`). */
export function restoredTexts(result: unknown): { steering: string[]; followUp: string[] } {
  const texts = (list: unknown) =>
    Array.isArray(list)
      ? list.flatMap((entry) => (isObject(entry) && typeof entry.text === "string" ? [entry.text] : []))
      : [];
  return isObject(result)
    ? { steering: texts(result.steering), followUp: texts(result.followUp) }
    : { steering: [], followUp: [] };
}

/** An image attachment from pasted or dropped bytes; `undefined` for anything but `image/*` (UI-060). */
export function imageAttachment(mime: string, bytes: Uint8Array): ImageContent | undefined {
  return mime.startsWith("image/") ? { type: "image", data: base64(bytes), mimeType: mime } : undefined;
}

/** Standard base64 with padding. */
export function base64(bytes: Uint8Array): string {
  const chunk = 0x8000;
  let binary = "";
  for (let start = 0; start < bytes.length; start += chunk) {
    binary += String.fromCharCode(...bytes.subarray(start, start + chunk));
  }
  return btoa(binary);
}
