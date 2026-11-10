/**
 * The transcript as a flat list of items (messages, inline notices, the streaming reply) and the display blocks of
 * each message. Items only hold indices into the thread state, so building the list stays cheap for long transcripts
 * and the virtualized list (UI-042) renders the visible ones only.
 */

import type { AgentMessage, AssistantMessage, ThreadState, ToolResultMessage, UserContent } from "@omfp/core";
import { strings } from "../strings/index.ts";
import { isObject, oneLine, prettyJson, type ToolCard, toolCard } from "./tool.ts";

/** What an item renders: `state.transcript[index]`, `state.notices[index]` or `state.streaming`. */
export type ItemKind =
  | { readonly type: "message"; readonly index: number }
  | { readonly type: "notice"; readonly index: number }
  | { readonly type: "streaming" };

/** One row of the transcript with a key that stays stable while the transcript grows. */
export interface Item {
  readonly key: string;
  readonly kind: ItemKind;
}

/**
 * The transcript's rows in order: each notice goes after the messages that preceded it, tool results shown inside
 * their call's card are left out, and the streaming reply comes last.
 */
export function items(state: ThreadState): Item[] {
  const called = new Set<string>();
  for (const { message } of state.transcript) {
    if (message.role === "assistant") {
      for (const block of message.content ?? []) {
        if (block.type === "toolCall" && block.id !== undefined) {
          called.add(block.id);
        }
      }
    }
  }
  const result: Item[] = [];
  let notice = 0;
  const noticesUpTo = (position: number) => {
    for (; notice < state.notices.length && (state.notices[notice]?.position ?? 0) <= position; notice++) {
      result.push({ key: `n${notice}`, kind: { type: "notice", index: notice } });
    }
  };
  for (const [index, { message }] of state.transcript.entries()) {
    noticesUpTo(index);
    if (message.role === "toolResult" && message.toolCallId !== undefined && called.has(message.toolCallId)) {
      continue;
    }
    result.push({ key: `m${index}`, kind: { type: "message", index } });
  }
  noticesUpTo(Number.POSITIVE_INFINITY);
  if (state.streaming !== null) {
    result.push({ key: "streaming", kind: { type: "streaming" } });
  }
  return result;
}

/** Who wrote a message; `record` for tool results without a visible call, bash executions, summaries, … */
export type Role = "user" | "assistant" | "record";

/** One displayed part of a message. */
export type Block =
  /** Markdown for the assistant (UI-040), plain text for everyone else. */
  | { readonly type: "text"; readonly text: string }
  /** Collapsed by default (UI-041). */
  | { readonly type: "thinking"; readonly text: string }
  | { readonly type: "image"; readonly mime: string; readonly data: string }
  | { readonly type: "tool"; readonly card: ToolCard }
  /** A message kind without its own renderer: a one-line summary and its JSON. */
  | { readonly type: "record"; readonly summary: string; readonly json: string };

/** A message ready to render. */
export interface MessageView {
  readonly role: Role;
  /** `You`, `Assistant`, or the record kind. */
  readonly label: string;
  readonly blocks: readonly Block[];
  /** The provider's error when the reply failed. */
  readonly error: string | undefined;
  /** A user message sent as steering. */
  readonly steering: boolean;
}

/** The view of `state.transcript[index]`. */
export function messageView(state: ThreadState, index: number): MessageView {
  const message = state.transcript[index]?.message;
  if (message === undefined) {
    return { role: "record", label: strings.chat.missingMessage, blocks: [], error: undefined, steering: false };
  }
  switch (message.role) {
    case "user":
      return {
        role: "user",
        label: strings.chat.roleUser,
        blocks: contentBlocks(message.content),
        error: undefined,
        steering: message.steering === true,
      };
    case "assistant":
      return assistantView(state, message);
    default:
      return recordView(message);
  }
}

/** The view of the reply being streamed, if any. */
export function streamingView(state: ThreadState): MessageView | undefined {
  return state.streaming === null ? undefined : assistantView(state, state.streaming.message);
}

function assistantView(state: ThreadState, message: AssistantMessage): MessageView {
  const completedAt = typeof message.completedAt === "number" ? message.completedAt : undefined;
  const calledAt = completedAt ?? message.timestamp;
  const blocks = (message.content ?? []).flatMap((block): Block[] => {
    switch (block.type) {
      case "text":
        return [{ type: "text", text: block.text ?? "" }];
      case "thinking":
        return [{ type: block.narration === true ? "text" : "thinking", text: block.thinking ?? "" }];
      case "redactedThinking":
        return [{ type: "thinking", text: strings.chat.redactedThinking }];
      case "image":
        return [{ type: "image", mime: block.mimeType ?? "image/png", data: block.data ?? "" }];
      case "toolCall": {
        const id = block.id ?? "";
        const run = state.tools.find((tool) => tool.tool_call_id === id);
        const result = state.transcript.find(
          (entry): entry is { message_id: string | null; message: ToolResultMessage } =>
            entry.message.role === "toolResult" && entry.message.toolCallId === id,
        )?.message;
        return [{ type: "tool", card: toolCard(id, block, run, result, calledAt) }];
      }
      default:
        return [];
    }
  });
  return {
    role: "assistant",
    label: strings.chat.roleAssistant,
    blocks,
    error: message.errorMessage,
    steering: false,
  };
}

function contentBlocks(content: string | readonly UserContent[] | undefined): Block[] {
  if (content === undefined) {
    return [];
  }
  if (typeof content === "string") {
    return [{ type: "text", text: content }];
  }
  return content.map((part) =>
    part.type === "text"
      ? { type: "text", text: part.text ?? "" }
      : { type: "image", mime: part.mimeType ?? "image/png", data: part.data ?? "" },
  );
}

/** Fields whose value summarizes a record, by preference. */
const SUMMARY_KEYS = ["content", "summary", "command", "code", "output", "text"] as const;

/** A generic row for message kinds without their own renderer: kind label, one-line summary, JSON. */
function recordView(message: AgentMessage): MessageView {
  const fields: Readonly<Record<string, unknown>> = message;
  const key = SUMMARY_KEYS.find((name) => fields[name] !== undefined);
  const summary = key === undefined ? "" : textOf(fields[key]);
  return {
    role: "record",
    label: strings.chat.records[message.role] ?? message.role,
    blocks: [{ type: "record", summary: oneLine(summary, 200), json: prettyJson(message) }],
    error: undefined,
    steering: false,
  };
}

/** A string, or the text parts of a content list, or compact JSON. */
function textOf(value: unknown): string {
  if (typeof value === "string") {
    return value;
  }
  if (Array.isArray(value)) {
    return value.flatMap((part) => (isObject(part) && typeof part.text === "string" ? [part.text] : [])).join(" ");
  }
  return JSON.stringify(value) ?? "";
}
