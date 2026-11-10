/**
 * The access token on the web (UI-002): `omfp daemon` prints the UI's URL with `#token=…`; the page reads it, keeps it
 * in `localStorage`, and removes it from the address bar (`history.replaceState`) so it does not end up in history,
 * bookmarks or screen shares. Fragments never reach the server (PRT-002).
 */

/** `localStorage` key of the access token. */
export const TOKEN_KEY = "omfp.token";

/** The `token` parameter of a URL fragment (`#token=…`, possibly among other `&`-separated parameters). */
export function fragmentToken(fragment: string): string | undefined {
  for (const parameter of parameters(fragment)) {
    if (parameter.startsWith("token=")) {
      const token = parameter.slice("token=".length).trim();
      return token === "" ? undefined : token;
    }
  }
  return undefined;
}

/** The fragment without its `token` parameter: `""` when nothing else is left, else `#` and the other parameters. */
export function fragmentWithoutToken(fragment: string): string {
  const rest = parameters(fragment).filter((parameter) => parameter !== "token" && !parameter.startsWith("token="));
  return rest.length === 0 ? "" : `#${rest.join("&")}`;
}

function parameters(fragment: string): string[] {
  return (fragment.startsWith("#") ? fragment.slice(1) : fragment).split("&").filter((parameter) => parameter !== "");
}

/** The token to connect with: a fresh one from the address bar wins over the stored one. */
export function chooseToken(fromFragment: string | undefined, stored: string | null | undefined): string | undefined {
  return fromFragment ?? (stored === null || stored === undefined || stored.trim() === "" ? undefined : stored);
}

/** What {@link takeToken} needs from the page. */
export interface TokenPage {
  readonly location: Pick<Location, "hash" | "pathname" | "search">;
  readonly history: Pick<History, "replaceState">;
  readonly storage: Pick<Storage, "getItem" | "setItem"> | undefined;
}

/**
 * The token to start with: `#token=…` (stored, then removed from the address bar with `history.replaceState`), else
 * the stored one.
 */
export function takeToken(page: TokenPage): string | undefined {
  const { location } = page;
  const fresh = fragmentToken(location.hash);
  if (fresh !== undefined) {
    page.storage?.setItem(TOKEN_KEY, fresh);
    page.history.replaceState(null, "", `${location.pathname}${location.search}${fragmentWithoutToken(location.hash)}`);
  }
  return chooseToken(fresh, page.storage?.getItem(TOKEN_KEY));
}
