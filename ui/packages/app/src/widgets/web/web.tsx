/**
 * `omfp:web` (UI-670): a web page — a dashboard, docs, a local dev server's preview — in the kit's `WebFrame`
 * (UI-671..UI-675). Props: `url`, an absolute `http(s)` URL other than the daemon's own origin; `title`, the tab's
 * label and the frame's accessible title. Agents place it with
 * `omfp_ui_mount { id, widget: "omfp:web", props: { url } }`.
 */

import { WebFrame } from "omfp/kit";
import type { WidgetProps } from "omfp/sdk";
import type { ReactNode } from "react";

export function WebWidget({ url, title }: WidgetProps): ReactNode {
  return <WebFrame url={url} title={typeof title === "string" ? title : undefined} />;
}
