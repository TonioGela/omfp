/**
 * The thread header (UI-030): title (placeholder until named, DMN-073), the workspace chip (UI-340), model and
 * thinking options, context usage, session cost, and process state with its action; archive state (UI-026).
 */

import type { ThreadState } from "@omfp/core";
import type { ThreadSummary, Workspace } from "@omfp/protocol";
import { isObject } from "../sdk/json.ts";
import { strings } from "../strings/index.ts";
import { needsLogin } from "./login.ts";
import { type PendingRestart, pendingRestart } from "./mcp.ts";
import {
  displayTitle,
  isArchived,
  type ThreadStatus,
  threadStatus,
  type WorkspaceChip,
  workspaceChip,
} from "./threads.ts";

/** A model as `thread.models` and `thread.state` describe it (the fields the header uses). */
export interface ModelInfo {
  readonly id: string;
  readonly name: string;
  readonly provider: string;
}

/** `thread.state`'s fields the header uses. */
export interface SessionDetails {
  readonly model: ModelInfo | undefined;
  readonly thinkingLevel: string | undefined;
  readonly contextUsage: { readonly percent: number; readonly contextWindow: number } | undefined;
}

/** Thread data fetched by calls rather than kept by the reducer. */
export interface ThreadDetails {
  /** `thread.state`. */
  readonly session: SessionDetails | undefined;
  /** `thread.models`. */
  readonly models: readonly ModelInfo[];
  /** `thread.models` answered: an empty {@link ThreadDetails.models} is then omp's (UI-388). */
  readonly modelsLoaded: boolean;
  /** `thread.thinking_levels`. */
  readonly thinkingLevels: readonly string[];
}

export const NO_DETAILS: ThreadDetails = { session: undefined, models: [], modelsLoaded: false, thinkingLevels: [] };

/** One model choice: `value` is `provider/id`. */
export interface ModelOption {
  readonly value: string;
  readonly label: string;
}

/** What the process can do next: stop a running or starting one, restart a stopped or failed one. */
export type ProcessAction = "stop" | "restart";

export interface HeaderView {
  readonly title: string;
  /** The title is the "New thread" placeholder (DMN-073). */
  readonly placeholder: boolean;
  /** The thread's workspace; `undefined` while it is unknown. */
  readonly workspace: WorkspaceChip | undefined;
  /** `provider/id` of the current model. */
  readonly model: string | undefined;
  /** The available models; the current one is included even when the list has not arrived. */
  readonly models: readonly ModelOption[];
  readonly thinking: string | undefined;
  readonly thinkingLevels: readonly string[];
  /** `42% of 200k`. */
  readonly context: string | undefined;
  /** Context tokens used and the window, for the token meter (UI-158). */
  readonly usage: { readonly used: number; readonly total: number } | undefined;
  readonly status: ThreadStatus;
  /** Why the process failed. */
  readonly failure: string | undefined;
  readonly action: ProcessAction | undefined;
  /** The header offers unarchive instead of archive (UI-026). */
  readonly archived: boolean;
  /** The daemon holds a restart while background work runs: offer it, named by its reasons (UI-349, PRT-134). */
  readonly restart: PendingRestart | undefined;
  /** The process failed because omp has no model it can use without a login or a key: offer Log in (UI-388). */
  readonly login: boolean;
  /** omp offers no model at all: the model list offers Log in (UI-388). */
  readonly noModels: boolean;
}

/** The header of a thread; the session cost comes from {@link costLabel}, so the header does not depend on the
 * reducer state, which changes with every streamed delta. */
export function headerView(
  summary: ThreadSummary,
  details: ThreadDetails,
  workspaces: readonly Workspace[],
): HeaderView {
  const current = details.session?.model;
  const model = current === undefined ? undefined : modelValue(current);
  const models = details.models.map(modelOption);
  if (current !== undefined && !models.some((option) => option.value === model)) {
    models.unshift(modelOption(current));
  }
  const thinking = details.session?.thinkingLevel;
  const thinkingLevels = [...details.thinkingLevels];
  if (thinking !== undefined && !thinkingLevels.includes(thinking)) {
    thinkingLevels.unshift(thinking);
  }
  const usage = details.session?.contextUsage;
  const process = summary.process;
  return {
    title: displayTitle(summary),
    placeholder: summary.titleSource === "placeholder" || summary.title.trim() === "",
    workspace: workspaceChip(summary.workspaceId, workspaces),
    model,
    models,
    thinking,
    thinkingLevels,
    context:
      usage === undefined
        ? undefined
        : strings.shell.header.context(Math.round(usage.percent), formatTokens(usage.contextWindow)),
    usage:
      usage === undefined
        ? undefined
        : { used: Math.round((usage.percent / 100) * usage.contextWindow), total: usage.contextWindow },
    status: threadStatus(summary),
    failure: process.state === "failed" ? process.message : undefined,
    action: processAction(process.state),
    archived: isArchived(summary),
    restart: pendingRestart(summary.restartPending),
    login: process.state === "failed" && (needsLogin(process.message) || needsLogin(process.stderr_tail)),
    noModels: details.modelsLoaded && models.length === 0,
  };
}

function processAction(state: ThreadSummary["process"]["state"]): ProcessAction {
  switch (state) {
    case "running":
    case "starting":
      return "stop";
    case "stopped":
    case "failed":
      return "restart";
  }
}

function modelValue(model: ModelInfo): string {
  return `${model.provider}/${model.id}`;
}

function modelOption(model: ModelInfo): ModelOption {
  return { value: modelValue(model), label: `${model.name} (${model.provider})` };
}

/** Totals computed per transcript array: the header renders on every event, the transcript changes per message. */
const COSTS = new WeakMap<ThreadState["transcript"], number>();

/** Total cost reported by the assistant messages of the transcript (`usage.cost.total`). */
export function sessionCost(state: ThreadState): number {
  const known = COSTS.get(state.transcript);
  if (known !== undefined) {
    return known;
  }
  let total = 0;
  for (const { message } of state.transcript) {
    if (message.role !== "assistant") {
      continue;
    }
    const cost = record(record(message.usage)?.cost)?.total;
    if (typeof cost === "number") {
      total += cost;
    }
  }
  COSTS.set(state.transcript, total);
  return total;
}

/** The session cost to show (`$0.0123`), `undefined` while nothing was spent. */
export function costLabel(state: ThreadState | undefined): string | undefined {
  const cost = state === undefined ? 0 : sessionCost(state);
  return cost > 0 ? formatCost(cost) : undefined;
}

/** `$0.0123` below a dollar, `$1.23` above. */
export function formatCost(cost: number): string {
  return cost < 1 ? `$${cost.toFixed(4)}` : `$${cost.toFixed(2)}`;
}

/** `200k`, `1.2M`, `950`. */
export function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000) {
    return `${Math.floor(tokens / 1_000_000)}.${Math.floor((tokens % 1_000_000) / 100_000)}M`;
  }
  return tokens >= 1000 ? `${Math.floor(tokens / 1000)}k` : String(tokens);
}

/** The model choice `provider/id` split in two (the id may contain slashes). */
export function splitModel(value: string): { provider: string; modelId: string } | undefined {
  const slash = value.indexOf("/");
  if (slash <= 0 || slash === value.length - 1) {
    return undefined;
  }
  return { provider: value.slice(0, slash), modelId: value.slice(slash + 1) };
}

/** `thread.models`' result (`{ models: ModelInfo[] }`); malformed entries are skipped. */
export function decodeModels(result: unknown): ModelInfo[] {
  const models = record(result)?.models;
  return Array.isArray(models) ? models.flatMap((model) => decodeModel(model) ?? []) : [];
}

/** `thread.thinking_levels`' result (`{ levels: string[] }`). */
export function decodeThinkingLevels(result: unknown): string[] {
  const levels = record(result)?.levels;
  return Array.isArray(levels) ? levels.filter((level): level is string => typeof level === "string") : [];
}

/** `thread.state`'s result (omp's `SessionState`). */
export function decodeSession(result: unknown): SessionDetails {
  const session = record(result);
  const usage = record(session?.contextUsage);
  const percent = usage?.percent;
  const contextWindow = usage?.contextWindow;
  const thinkingLevel = session?.thinkingLevel;
  return {
    model: decodeModel(session?.model),
    thinkingLevel: typeof thinkingLevel === "string" ? thinkingLevel : undefined,
    contextUsage:
      typeof percent === "number" && typeof contextWindow === "number" ? { percent, contextWindow } : undefined,
  };
}

function decodeModel(value: unknown): ModelInfo | undefined {
  const model = record(value);
  const { id, name, provider } = model ?? {};
  return typeof id === "string" && typeof name === "string" && typeof provider === "string"
    ? { id, name, provider }
    : undefined;
}

/** `value` when it is an object, so optional chains reach into untyped results. */
function record(value: unknown): Readonly<Record<string, unknown>> | undefined {
  return isObject(value) ? value : undefined;
}
