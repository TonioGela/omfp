/** A thread's state badge (UI-020) and the workspace/repository chips (UI-020, UI-030), shared by the shell widgets. */

import { Badge, Chip, type Tone } from "@omfp/kit";
import type { ReactNode } from "react";
import { strings } from "../../strings/index.ts";
import { statusGlyph, statusLabel, type ThreadStatus, type WorkspaceChip } from "../../vm/threads.ts";

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

/** The workspace's name, marked as the thread's cwd, or a repository's `name:branch` with its kind. */
export function WorkspaceChipView({ chip }: { chip: WorkspaceChip }): ReactNode {
  const workspace = chip.kind === "workspace";
  return (
    <Chip
      data-testid="workspace-chip"
      data-kind={chip.kind}
      title={workspace ? strings.shell.chip.workingDirectory : chip.kind}
    >
      {workspace && (
        <span className="omfp-chip__cwd" role="img" aria-label={strings.shell.chip.cwd}>
          ⌂
        </span>
      )}
      {chip.label}
    </Chip>
  );
}
