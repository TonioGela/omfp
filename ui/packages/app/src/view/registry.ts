/** The built-in `omfp:*` widgets by name (UI-024); M4 adds workspace and user widgets to the same map. */

import type { WidgetComponent } from "../sdk/index.ts";
import { ChatWidget, ComposerWidget, InteractionsWidget, TranscriptWidget } from "../widgets/chat/index.ts";
import { HeaderWidget } from "../widgets/header/index.ts";
import { ThreadsWidget } from "../widgets/threads/index.ts";
import { WorkspaceWidget } from "../widgets/workspace/index.ts";
import type { WidgetRegistry } from "./render.tsx";

export const BUILTIN_WIDGETS: WidgetRegistry = new Map<string, WidgetComponent>([
  ["omfp:threads", ThreadsWidget],
  ["omfp:header", HeaderWidget],
  ["omfp:workspace", WorkspaceWidget],
  ["omfp:chat", ChatWidget],
  ["omfp:transcript", TranscriptWidget],
  ["omfp:interactions", InteractionsWidget],
  ["omfp:composer", ComposerWidget],
]);

/** The widgets that render the selected thread's pending interactions; without one of them in the view, the frame's
 * safety net does (08-agent-control.md). */
export const INTERACTION_WIDGETS: readonly string[] = ["omfp:chat", "omfp:interactions"];
