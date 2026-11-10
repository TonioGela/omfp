/**
 * Sources for the kit's `PathField` (DMN-021): repositories the daemon has seen (`repos.suggest`) and directories on
 * the daemon host (`fs.complete`), so paths complete on the daemon's machine even when the browser is remote.
 */

import type { PathCompletion, PathSuggestion } from "@omfp/kit";
import { useMemo } from "react";
import { useCall } from "./hooks.ts";

export interface PathSources {
  readonly suggest: (query: string) => Promise<readonly PathSuggestion[]>;
  readonly complete: (path: string) => Promise<readonly PathCompletion[]>;
}

/** `suggest` and `complete` for a `PathField`, stable while the client is. */
export function usePathSources(): PathSources {
  const call = useCall();
  return useMemo(
    () => ({
      suggest: async (query: string) => (await call("repos.suggest", { query })).repos,
      complete: async (path: string) => (await call("fs.complete", { path })).entries,
    }),
    [call],
  );
}
