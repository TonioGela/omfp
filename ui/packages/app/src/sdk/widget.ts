/** What a widget is (docs/specs/08-agent-control.md): a React component taking its view node's props. */

import type { ComponentType } from "react";

/** The props of a widget reference in a view (`["omfp:changes", { "repo": "x" }]` → `{ repo: "x" }`). */
export type WidgetProps = Readonly<Record<string, unknown>>;

export type WidgetComponent = ComponentType<WidgetProps>;
