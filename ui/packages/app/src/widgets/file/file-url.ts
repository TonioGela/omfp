/**
 * The SDK's `useFileUrl(path)` (a workspace file's URL on the daemon's raw-file route) when this build of the SDK has
 * it, looked up at run time so the widget also runs on an SDK without it (relative images then show their alt text,
 * UI-562).
 */

import * as sdk from "omfp/sdk";

/** A workspace path (as `useFile` takes it) to the URL the daemon serves it at; `undefined` until there is one. */
export type UseFileUrl = (path: string) => string | undefined;

const found: unknown = (sdk as Readonly<Record<string, unknown>>).useFileUrl;

export const useFileUrl: UseFileUrl | undefined = typeof found === "function" ? (found as UseFileUrl) : undefined;
