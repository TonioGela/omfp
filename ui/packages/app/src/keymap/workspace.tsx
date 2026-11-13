/**
 * The current workspace's keymap in the window (UI-210, UI-212): the bindings of its `.omfp/keymap.json`, which the
 * daemon reads and pushes on the `views/<id>` topic (DMN-121, PRT-060), over the defaults and the daemon's `[keymap]`
 * entries; and the banner naming why the file is refused while its last valid bindings stay in force.
 */

import type { ViewsState } from "@omfp/protocol";
import { type ReactNode, useMemo } from "react";
import { strings } from "../strings/index.ts";
import { type Binding, effectiveBindings, type InitBindings, type KeymapOverride } from "./keymap.ts";

/** The window's bindings, live: `defaults` < `global` (`[keymap]`) < the bindings of `views`, the current
 * workspace's views topic (none while there is no current workspace or before its topic answers) < the init files'
 * (UI-262). */
export function useBindings(
  defaults: readonly Binding[],
  global: readonly KeymapOverride[],
  views: ViewsState | undefined,
  init: InitBindings,
): readonly Binding[] {
  const workspace = views?.keymap;
  return useMemo(() => effectiveBindings(defaults, global, workspace ?? [], init), [defaults, global, workspace, init]);
}

/** The error banner of the current workspace's `.omfp/keymap.json` (UI-212); nothing while it is valid or absent. */
export function KeymapError({ views }: { readonly views: ViewsState | undefined }): ReactNode {
  const error = views?.keymapError;
  if (error === undefined || error === null) {
    return null;
  }
  return (
    <div className="omfp-banner" data-tone="error" role="alert" data-testid="keymap-error">
      {strings.keys.invalid(error)}
    </div>
  );
}
