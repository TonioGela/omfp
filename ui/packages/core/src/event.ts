/**
 * Inputs of the thread reducer (port of `omfp-core/src/event.rs`): everything that changes a thread's visible state,
 * in the order the daemon applies them (ARC-006, DMN-040). The same sequence is replayed by clients and recorded as
 * fixtures (TST-012).
 */

import { bool, type Decoder, Invalid, i32, int, optional, record, str } from "./decode.ts";
import { isObject, type JsonValue } from "./json.ts";
import type { AgentMessage, RpcNotification } from "./wire.ts";

/** Lifecycle of the omp process behind a thread (02-daemon, "Process states"). */
export type ProcessState =
  | { state: "stopped" }
  | { state: "starting" }
  | { state: "running" }
  /** Exited unexpectedly or failed to start (DMN-035). `stderr_tail` (last lines of stderr) is left out when empty. */
  | { state: "failed"; message: string; exit_code?: number; stderr_tail?: string }
  /** The session is owned by an interactive omp in a terminal (05-terminal). */
  | { state: "terminal" };

/** The omp session a thread is bound to. */
export interface SessionInfo {
  session_id: string;
  session_file?: string;
  /** `true` when an existing session was resumed rather than created. */
  resumed: boolean;
}

/** Who answered an interactive request (DMN-051): a connected client, by its `system.hello` name. */
export interface Resolution {
  by: "client";
  client: string;
}

/**
 * Who started a live voice session (DMN-100): the daemon's number of the connection (the `connection` of its
 * `system.hello` result, PRT-051) and that connection's `system.hello` client name.
 */
export interface LiveOwner {
  client: string;
  connection: number;
}

/** One input of the thread reducer, discriminated by `kind`. */
export type ThreadEvent =
  /** A frame from omp that is not a command response. */
  | { kind: "omp"; frame: RpcNotification }
  /** Transcript of the session right after the process started (DMN-032). Replaces the transcript. */
  | { kind: "history"; session: SessionInfo; messages: AgentMessage[] }
  /** Process lifecycle change. */
  | { kind: "process"; state: ProcessState }
  /** The daemon forwarded an answer to the interactive request with omp id `id` (DMN-051). */
  | { kind: "interaction_resolved"; id: string; resolution: Resolution }
  /**
   * The daemon opened a live voice session for `owner` before forwarding `live_connect` (DMN-100), or closed one omp
   * never started (`null`, DMN-101). omp's `live_*` frames arrive as `omp` events.
   */
  | { kind: "live_session"; owner: LiveOwner | null }
  /** A frame omp-rpc could not decode, or a response nobody waited for (DMN-042). */
  | { kind: "unknown"; raw: JsonValue };

const failed = record<{ message: string; exit_code?: number; stderr_tail?: string }>({
  message: str,
  exit_code: optional(i32),
  stderr_tail: optional(str),
});

export const processState: Decoder<ProcessState> = (value) => {
  const state = isObject(value) ? value.state : undefined;
  switch (state) {
    case "stopped":
    case "starting":
    case "running":
    case "terminal":
      return { state };
    case "failed": {
      const { stderr_tail, ...rest } = failed(value);
      return stderr_tail === undefined || stderr_tail === "" ? { state, ...rest } : { state, ...rest, stderr_tail };
    }
    default:
      throw new Invalid("unknown ProcessState");
  }
};

export const sessionInfo = record<SessionInfo>({ session_id: str, session_file: optional(str), resumed: bool });

export const resolution = record<Resolution>({
  by: (value) => {
    if (value === "client") {
      return value;
    }
    throw new Invalid("unknown Resolution");
  },
  client: str,
});

/** A `u64` connection number. */
const connectionNumber: Decoder<number> = (value) => {
  const decoded = int(value);
  if (decoded < 0) {
    throw new Invalid("negative connection number");
  }
  return decoded;
};

export const liveOwner = record<LiveOwner>({ client: str, connection: connectionNumber });
