/** Window state slots shared by several widgets and the frame. Widget-private slots live next to their widget. */

import type { ImageContent } from "@omfp/core";
import { type WindowKey, windowKey } from "./window-store.ts";

/** The thread this window shows; `undefined` for none. */
export const SELECTED_THREAD = windowKey<string | undefined>("selectedThread", undefined);

/** A composer draft: text and pasted or dropped images (UI-060, UI-064). */
export interface Draft {
  readonly text: string;
  readonly images: readonly ImageContent[];
}

export const EMPTY_DRAFT: Draft = { text: "", images: [] };

/** The draft of one thread, kept across thread switches and reconnects (UI-064); dropped when the thread is archived
 * or deleted (UI-026). */
export function draftKey(threadId: string): WindowKey<Draft> {
  let key = DRAFT_KEYS.get(threadId);
  if (key === undefined) {
    key = windowKey<Draft>(`draft/${threadId}`, EMPTY_DRAFT);
    DRAFT_KEYS.set(threadId, key);
  }
  return key;
}

/** One key object per thread, so hooks see a stable key. */
const DRAFT_KEYS = new Map<string, WindowKey<Draft>>();

/** Manual colour scheme override (UI-091), per window; `system` follows `prefers-color-scheme`. Density is a daemon
 * setting (`useSettings`, UI-097). */
export type ColorScheme = "system" | "light" | "dark";

export const COLOR_SCHEME = windowKey<ColorScheme>("colorScheme", "system", (stored) =>
  stored === "light" || stored === "dark" || stored === "system" ? stored : undefined,
);

/** Id of the view this window chose (UI-101); `undefined`: the manifest's default. */
export const ACTIVE_VIEW = windowKey<string | undefined>("activeView", undefined);

/** A thread being started in a workspace (UI-023): shown while no thread is selected; the daemon thread is created
 * when its first message is sent. One per window. */
export interface DraftThread {
  readonly workspaceId: string;
}

export const DRAFT_THREAD = windowKey<DraftThread | undefined>("draftThread", undefined);

/** The composer text of the {@link DRAFT_THREAD}; dropped with it. */
export const DRAFT_MESSAGE = windowKey<Draft>("draft/draftThread", EMPTY_DRAFT);

/** The workspace whose delete confirmation is open (UI-020, DMN-028). */
export const DELETE_WORKSPACE = windowKey<string | undefined>("deleteWorkspace", undefined);

/** The thread whose delete confirmation is open (UI-026). */
export const DELETE_THREAD = windowKey<string | undefined>("deleteThread", undefined);

/** The thread whose title is being edited in place (UI-030). */
export const RENAMING_THREAD = windowKey<string | undefined>("renamingThread", undefined);

/** Errors of failed actions, oldest first, shown by the frame until dismissed. */
export const ERRORS = windowKey<readonly string[]>("errors", []);

/** A request to show a built-in widget (UI-274): the frame selects the tab holding `reference` in the current view,
 * else floats it in a panel of this window; `nonce` tells one request from the next. */
export interface RevealRequest {
  readonly reference: string;
  readonly nonce: number;
}

export const REVEAL_WIDGET = windowKey<RevealRequest | undefined>("revealWidget", undefined);

/** A request to open a subagent's transcript in `omfp:subagents` (UI-271); `nonce` tells one request from the next. */
export interface OpenSubagentRequest {
  readonly threadId: string;
  readonly subagentId: string;
  readonly nonce: number;
}

export const OPEN_SUBAGENT = windowKey<OpenSubagentRequest | undefined>("openSubagent", undefined);
