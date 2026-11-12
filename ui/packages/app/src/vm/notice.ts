/**
 * Inline notice rows (UI-043): compaction, retries, omp notices, extension and frame errors as compact rows; frames
 * the reducer could not interpret as a generic row with expandable JSON.
 */

import type { NoticeFrame } from "@omfp/core";
import { isObject } from "../sdk/json.ts";
import { strings } from "../strings/index.ts";
import { prettyJson } from "./tool.ts";

export type Level = "info" | "warning" | "error";

/** Stable kind token of a row. */
export type NoticeKind = "compaction" | "retry" | "notice" | "extension-error" | "frame-error" | "unknown";

export interface NoticeRow {
  readonly kind: NoticeKind;
  readonly level: Level;
  readonly text: string;
  /** Pretty JSON of an unknown frame, shown on demand. */
  readonly json: string | undefined;
}

const text = strings.chat.notice;

function row(kind: NoticeKind, level: Level, message: string): NoticeRow {
  return { kind, level, text: message, json: undefined };
}

/** The row of a notice frame. */
export function noticeRow(frame: NoticeFrame): NoticeRow {
  switch (frame.type) {
    case "auto_compaction_start":
      return row("compaction", "info", text.compacting(frame.data.reason, frame.data.action));
    case "manual_compaction_start":
      return row("compaction", "info", text.compactingManual);
    case "auto_compaction_end":
    case "manual_compaction_end":
      return compactionEnd(frame.data.aborted, frame.data.skipped, frame.data.errorMessage);
    case "auto_retry_start": {
      const { attempt, maxAttempts, delayMs, errorMessage } = frame.data;
      return row("retry", "warning", text.retrying(attempt, maxAttempts, delayMs / 1000, errorMessage));
    }
    case "auto_retry_end":
      return frame.data.success
        ? row("retry", "info", text.retrySucceeded(frame.data.attempt))
        : row("retry", "error", text.retryFailed(frame.data.attempt, frame.data.finalError ?? text.unknownError));
    case "notice": {
      const { level, message, source } = frame.data;
      return row("notice", level, source === undefined ? message : text.sourced(source, message));
    }
    case "extension_error": {
      const { extensionPath, event, error } = frame.data;
      return row("extension-error", "error", text.extensionError(extensionPath, event, error));
    }
    case "rpc_frame_error": {
      const { originalType, error } = frame.data;
      return row(
        "frame-error",
        "error",
        originalType === undefined ? text.frameError(error) : text.frameErrorOf(originalType, error),
      );
    }
    case "unknown": {
      const kind = isObject(frame.data) ? frame.data.type : undefined;
      return {
        kind: "unknown",
        level: "info",
        text: typeof kind === "string" ? text.unrecognizedOf(kind) : text.unrecognized,
        json: prettyJson(frame.data),
      };
    }
  }
}

function compactionEnd(aborted: boolean, skipped: boolean | undefined, error: string | undefined): NoticeRow {
  if (error !== undefined) {
    return row("compaction", "error", text.compactionFailed(error));
  }
  if (aborted) {
    return row("compaction", "warning", text.compactionAborted);
  }
  return row("compaction", "info", skipped === true ? text.compactionSkipped : text.compacted);
}
