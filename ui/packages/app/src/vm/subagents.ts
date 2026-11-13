/**
 * A thread's subagents (UI-081), which `omfp:background` lists and opens (UI-490): the roster from the reducer's
 * subagent frames (`subagents`, PRT-055) merged with omp's `thread.subagents` (only running ones: omp drops finished
 * subagents from it), and a subagent's transcript read incrementally with `thread.subagent_messages` (`fromByte`).
 */

import type { AgentMessage, JsonObject, Subagent } from "@omfp/core";
import { compactNumber } from "@omfp/kit";
import { isObject } from "../sdk/json.ts";
import { strings } from "../strings/index.ts";
import { contentText } from "./tree.ts";

const text = strings.shell.subagents;

/** One subagent of the roster. */
export interface SubagentRow {
  readonly id: string;
  readonly index: number;
  /** The agent kind (`task`, `scout`, `widget-builder`, …). */
  readonly agent: string;
  /** omp's status: `running`, `completed`, `failed`, `aborted`, or another it may add. */
  readonly status: string;
  readonly description: string | undefined;
  /** What it was asked to do (its assignment, else the whole task). */
  readonly task: string;
  /** What it does now (its current tool's intent), else what it did last. */
  readonly activity: string | undefined;
  /** `3 tools · 12.3k tokens · fake/cheap`, what is known of it. */
  readonly meta: string;
  /** Its run time so far (`durationMs` of its progress), when known. */
  readonly durationMs: number | undefined;
  /** Still running: it can be steered and cancelled. */
  readonly live: boolean;
}

/** The lifecycle statuses after which a subagent never runs again. */
const FINISHED: Readonly<Record<string, true>> = { completed: true, failed: true, aborted: true };

function str(value: unknown): string | undefined {
  return typeof value === "string" && value !== "" ? value : undefined;
}

function num(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

/** A `thread.subagents` snapshot as far as the roster reads it. */
interface Listed {
  readonly id: string;
  readonly index: number;
  readonly agent: string;
  readonly status: string;
  readonly description: string | undefined;
  readonly task: string | undefined;
  readonly progress: JsonObject | undefined;
}

/** The snapshots of a `thread.subagents` result (`{ subagents }`); entries without an id are skipped. */
function listedSubagents(result: unknown): Listed[] {
  const list = isObject(result) && Array.isArray(result.subagents) ? result.subagents : [];
  return list.flatMap((entry: unknown) => {
    if (!isObject(entry) || typeof entry.id !== "string") {
      return [];
    }
    return [
      {
        id: entry.id,
        index: num(entry.index) ?? 0,
        agent: str(entry.agent) ?? "",
        status: str(entry.status) ?? "running",
        description: str(entry.description),
        task: str(entry.assignment) ?? str(entry.task),
        progress: isObject(entry.progress) ? (entry.progress as JsonObject) : undefined,
      },
    ];
  });
}

function meta(progress: JsonObject | undefined): string {
  if (progress === undefined) {
    return "";
  }
  const tools = num(progress.toolCount) ?? (Array.isArray(progress.recentTools) ? progress.recentTools.length : 0);
  const tokens = num(progress.tokens);
  return [
    text.tools(tools),
    tokens === undefined || tokens === 0 ? undefined : text.tokens(compactNumber(tokens)),
    str(progress.resolvedModel),
  ]
    .filter((part) => part !== undefined)
    .join(" · ");
}

function row(id: string, known: Subagent | undefined, listed: Listed | undefined): SubagentRow {
  const lifecycle = known?.lifecycle ?? null;
  const progress = known?.progress?.progress ?? listed?.progress;
  const finished = lifecycle !== null && Object.hasOwn(FINISHED, lifecycle.status) ? lifecycle.status : undefined;
  const status = finished ?? str(progress?.status) ?? listed?.status ?? "running";
  return {
    id,
    index: lifecycle?.index ?? known?.progress?.index ?? listed?.index ?? 0,
    agent: lifecycle?.agent ?? known?.progress?.agent ?? listed?.agent ?? "",
    status,
    description: lifecycle?.description ?? str(progress?.description) ?? listed?.description,
    task: known?.progress?.assignment ?? listed?.task ?? known?.progress?.task ?? "",
    activity: str(progress?.currentToolIntent) ?? str(progress?.lastIntent),
    meta: meta(progress),
    durationMs: num(progress?.durationMs),
    live: finished === undefined && !Object.hasOwn(FINISHED, status),
  };
}

/** The roster: every subagent the thread's state knows, plus running ones only `thread.subagents` reports (e.g.
 * started before the daemon restarted), by spawn index. */
export function subagentRows(subagents: Readonly<Record<string, Subagent>>, listed: unknown): SubagentRow[] {
  const snapshots = new Map(listedSubagents(listed).map((entry) => [entry.id, entry]));
  const ids = [...new Set([...Object.keys(subagents), ...snapshots.keys()])];
  return ids
    .map((id) => row(id, subagents[id], snapshots.get(id)))
    .sort((a, b) => a.index - b.index || a.id.localeCompare(b.id));
}

/** A subagent's transcript as read so far. */
export interface SubagentTranscript {
  readonly messages: readonly AgentMessage[];
  /** Where the next read starts (`fromByte`). */
  readonly nextByte: number;
}

export const EMPTY_TRANSCRIPT: SubagentTranscript = { messages: [], nextByte: 0 };

/**
 * `transcript` with one `thread.subagent_messages` page: its messages appended, or replacing them when omp restarted
 * from the start of the file (`reset`). A page that does not decode leaves it unchanged.
 */
export function appendTranscriptPage(transcript: SubagentTranscript, page: unknown): SubagentTranscript {
  if (!isObject(page) || !Array.isArray(page.messages) || typeof page.nextByte !== "number") {
    return transcript;
  }
  const messages = page.messages.filter(
    (message: unknown): message is AgentMessage => isObject(message) && typeof message.role === "string",
  );
  if (page.reset === true) {
    return { messages, nextByte: page.nextByte };
  }
  if (messages.length === 0 && page.nextByte === transcript.nextByte) {
    return transcript;
  }
  return { messages: [...transcript.messages, ...messages], nextByte: page.nextByte };
}

/** One line of a subagent's transcript. */
export interface TranscriptLine {
  readonly role: "user" | "assistant" | "tool";
  readonly text: string;
}

/** The lines a transcript shows: user and assistant text, one line per tool call (its intent, else its name). */
export function transcriptLines(messages: readonly AgentMessage[]): TranscriptLine[] {
  return messages.flatMap((message): TranscriptLine[] => {
    if (message.role === "user") {
      const body = contentText(message.content).trim();
      return body === "" ? [] : [{ role: "user", text: body }];
    }
    if (message.role !== "assistant") {
      return [];
    }
    const blocks = Array.isArray(message.content) ? message.content : [];
    return blocks.flatMap((block): TranscriptLine[] => {
      if (block.type === "text" && typeof block.text === "string" && block.text.trim() !== "") {
        return [{ role: "assistant", text: block.text.trim() }];
      }
      if (block.type === "toolCall") {
        const intent = typeof block.intent === "string" ? block.intent : undefined;
        return [{ role: "tool", text: text.toolLine(block.name ?? "", intent) }];
      }
      return [];
    });
  });
}
