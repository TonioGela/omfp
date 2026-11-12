/**
 * The access token on the web (UI-002, UI-255): the URLs the daemon opens or prints carry a one-time `#ticket=…`
 * (DMN-186, experimental) that the page trades for the token in `system.hello` and never stores; older links and
 * hand-made ones carry `#token=…`. The page keeps the token in `localStorage` and removes both from the address bar
 * (`history.replaceState`) so they do not end up in history, bookmarks or screen shares. Fragments never reach the
 * server (PRT-002).
 *
 * `omfp desktop` adds `local=1` when the daemon runs on the window's machine (UI-188): it stays in the address bar and
 * is never stored, since it belongs to that window, not to the daemon's origin.
 */

import type { Credential } from "@omfp/client";
import { strings } from "../strings/index.ts";

/** `localStorage` key of the access token. */
export const TOKEN_KEY = "omfp.token";

/** The value of the fragment parameter `name` (`#name=…`, possibly among other `&`-separated parameters). */
function fragmentValue(fragment: string, name: string): string | undefined {
  for (const parameter of parameters(fragment)) {
    if (parameter.startsWith(`${name}=`)) {
      const value = parameter.slice(name.length + 1).trim();
      return value === "" ? undefined : value;
    }
  }
  return undefined;
}

/** The `token` parameter of a URL fragment (`#token=…`, possibly among other `&`-separated parameters). */
export function fragmentToken(fragment: string): string | undefined {
  return fragmentValue(fragment, "token");
}

/** The `ticket` parameter of a URL fragment (`#ticket=…`, UI-255). */
export function fragmentTicket(fragment: string): string | undefined {
  return fragmentValue(fragment, "ticket");
}

/** Whether the fragment says the window runs on the daemon's machine (`local=1`, UI-188). */
export function fragmentLocal(fragment: string): boolean {
  return parameters(fragment).includes("local=1");
}

/** The fragment without its `token` and `ticket` parameters: `""` when nothing else is left, else `#` and the rest. */
export function fragmentWithoutCredential(fragment: string): string {
  const rest = parameters(fragment).filter(
    (parameter) => !["token", "ticket"].some((name) => parameter === name || parameter.startsWith(`${name}=`)),
  );
  return rest.length === 0 ? "" : `#${rest.join("&")}`;
}

/**
 * The thread a window opens on (`#thread=<id>`, `omfp desktop --thread`, UI-218): read once, then removed from the
 * address bar (`history.replaceState`) so a reload keeps the window's own selection.
 */
export function takeThread(page: Pick<TokenPage, "location" | "history">): string | undefined {
  const { location } = page;
  const all = parameters(location.hash);
  const parameter = all.find((candidate) => candidate.startsWith("thread="));
  if (parameter === undefined) {
    return undefined;
  }
  const rest = all.filter((candidate) => !candidate.startsWith("thread="));
  page.history.replaceState(
    null,
    "",
    `${location.pathname}${location.search}${rest.length === 0 ? "" : `#${rest.join("&")}`}`,
  );
  let thread: string;
  try {
    thread = decodeURIComponent(parameter.slice("thread=".length)).trim();
  } catch {
    return undefined;
  }
  return thread === "" ? undefined : thread;
}

function parameters(fragment: string): string[] {
  return (fragment.startsWith("#") ? fragment.slice(1) : fragment).split("&").filter((parameter) => parameter !== "");
}

/** The token to connect with: a fresh one from the address bar wins over the stored one. */
export function chooseToken(fromFragment: string | undefined, stored: string | null | undefined): string | undefined {
  return fromFragment ?? (stored === null || stored === undefined || stored.trim() === "" ? undefined : stored);
}

/** What {@link takeCredential} needs from the page. */
export interface TokenPage {
  readonly location: Pick<Location, "hash" | "pathname" | "search">;
  readonly history: Pick<History, "replaceState">;
  readonly storage: Pick<Storage, "getItem" | "setItem"> | undefined;
}

/**
 * What to connect with, both removed from the address bar with `history.replaceState`: `#ticket=…` (never stored: the
 * token it is traded for is), else `#token=…` (stored), else the stored token.
 */
export function takeCredential(page: TokenPage): Credential | undefined {
  const { location } = page;
  const ticket = fragmentTicket(location.hash);
  const fresh = fragmentToken(location.hash);
  if (ticket !== undefined || fresh !== undefined) {
    page.history.replaceState(
      null,
      "",
      `${location.pathname}${location.search}${fragmentWithoutCredential(location.hash)}`,
    );
  }
  if (ticket !== undefined) {
    return { ticket };
  }
  if (fresh !== undefined) {
    page.storage?.setItem(TOKEN_KEY, fresh);
  }
  const token = chooseToken(fresh, page.storage?.getItem(TOKEN_KEY));
  return token === undefined ? undefined : { token };
}

/**
 * What the token screen says when the daemon refused `credential` with `message` (UI-255): a used ticket may have been
 * opened by someone else first, so the screen says what to do about it.
 */
export function rejection(credential: Credential, message: string): string {
  const text = strings.shell.token;
  if (!("ticket" in credential)) {
    return text.rejected;
  }
  return message === "ticket already used" ? text.ticketUsed : text.ticketExpired;
}
