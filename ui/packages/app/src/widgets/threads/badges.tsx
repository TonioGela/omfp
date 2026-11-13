/** A thread's state badge (UI-020), shared by the shell widgets. */

import { Badge, type Tone } from "@omfp/kit";
import type { ReactNode } from "react";
import { statusGlyph, statusLabel, type ThreadStatus } from "../../vm/threads.ts";

const TONES: Readonly<Record<ThreadStatus, Tone>> = {
  awaiting: "warning",
  failed: "error",
  running: "primary",
  starting: "primary",
  idle: "success",
  stopped: "default",
  terminal: "default",
};

export function StatusBadge({ status, testid }: { status: ThreadStatus; testid: string }): ReactNode {
  const label = statusLabel(status);
  return (
    <Badge tone={TONES[status]} data-testid={testid} data-status={status} title={label}>
      <span aria-hidden="true">{statusGlyph(status)}</span>
      <span className="omfp-visually-hidden">{label}</span>
    </Badge>
  );
}
