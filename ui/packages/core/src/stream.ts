/**
 * Rebuilding a streaming assistant message from `messageUpdates: "delta"` events (port of `omfp-core/src/stream.rs`;
 * omp `docs/rpc.md`, "Event Stream Schema"): `*_start` carries the block shell with its streamed content emptied,
 * deltas append to the block at `contentIndex`, and `*_end` is authoritative for that block. Every function returns a
 * new message and leaves its inputs untouched.
 */

import { isObject } from "./json.ts";
import type { StreamingMessage } from "./state.ts";
import {
  type AssistantContent,
  type AssistantMessage,
  type AssistantMessageEvent,
  assistantContent,
  assistantContentList,
} from "./wire.ts";

/** The block kinds that stream a string, by their `type`. */
type Streamed = "text" | "thinking" | "toolCall";

/** Key of the streamed string of each kind; a tool call's is omp's own name for its partial argument JSON. */
const STREAMED_KEY: { [K in Streamed]: string } = { text: "text", thinking: "thinking", toolCall: "partialArgs" };

/** An empty assistant message with id `messageId`. */
export function newStreamingMessage(messageId: string | null): StreamingMessage {
  return { message_id: messageId, message: { role: "assistant", content: [] } };
}

/** `block` with its streamed string set by `update` (from the current string, `""` when missing or not a string). */
function withStreamed(block: AssistantContent, kind: Streamed, update: (current: string) => string): AssistantContent {
  if (block.type !== kind) {
    return block;
  }
  const key = STREAMED_KEY[kind];
  const current = block[key];
  return { ...block, [key]: update(typeof current === "string" ? current : "") };
}

/** A `contentIndex` usable as an array index (omp-rpc: an `i64` that fits a `usize`). */
function slot(index: unknown): number | undefined {
  return typeof index === "number" && Number.isSafeInteger(index) && index >= 0 ? index : undefined;
}

/** Sets the block at `index`, appending when the index is past the end. Undecodable content starts over. */
function put(message: AssistantMessage, index: number, block: AssistantContent): AssistantMessage {
  const content = [...(assistantContentList(message.content) ?? [])];
  if (index < content.length) {
    content[index] = block;
  } else {
    content.push(block);
  }
  return { ...message, content };
}

/** `*_start`: the carried shell (or an empty block of `kind`) with its streamed string emptied. */
function begin(message: AssistantMessage, event: AssistantMessageEvent, kind: Streamed): AssistantMessage {
  const index = slot(event.contentIndex);
  if (index === undefined) {
    return message;
  }
  const block = assistantContent(event.block) ?? { type: kind };
  return put(
    message,
    index,
    withStreamed(block, kind, () => ""),
  );
}

/** `*_delta`: appends to the block at `index` when it is of `kind`. */
function append(message: AssistantMessage, event: AssistantMessageEvent, kind: Streamed): AssistantMessage {
  const delta = typeof event.delta === "string" ? event.delta : undefined;
  const index = slot(event.contentIndex);
  const content = assistantContentList(message.content);
  const block = index === undefined ? undefined : content?.[index];
  if (delta === undefined || index === undefined || block?.type !== kind) {
    return message;
  }
  return put(
    message,
    index,
    withStreamed(block, kind, (current) => current + delta),
  );
}

/** `text_end` / `thinking_end`: the carried shell (else the rebuilt block) with `content` as its text. */
function finish(message: AssistantMessage, event: AssistantMessageEvent, kind: Streamed): AssistantMessage {
  const index = slot(event.contentIndex);
  if (index === undefined) {
    return message;
  }
  const content = typeof event.content === "string" ? event.content : undefined;
  const block = assistantContent(event.block) ?? assistantContentList(message.content)?.[index] ?? { type: kind };
  return put(
    message,
    index,
    withStreamed(block, kind, (current) => content ?? current),
  );
}

/** Applies one delta-mode event. `done` and `error` carry nothing to rebuild: `message_end` follows. */
export function applyStreamEvent(streaming: StreamingMessage, event: AssistantMessageEvent): StreamingMessage {
  const message = streaming.message;
  let next: AssistantMessage = message;
  switch (event.type) {
    case "start":
      // omp-rpc decodes `message` as an open record: any object does.
      if (isObject(event.message)) {
        next = event.message as AssistantMessage;
      }
      break;
    case "text_start":
      next = begin(message, event, "text");
      break;
    case "thinking_start":
      next = begin(message, event, "thinking");
      break;
    case "toolcall_start":
      next = begin(message, event, "toolCall");
      break;
    case "text_delta":
      next = append(message, event, "text");
      break;
    case "thinking_delta":
      next = append(message, event, "thinking");
      break;
    case "toolcall_delta":
      next = append(message, event, "toolCall");
      break;
    case "text_end":
      next = finish(message, event, "text");
      break;
    case "thinking_end":
      next = finish(message, event, "thinking");
      break;
    case "image_end": {
      const index = slot(event.contentIndex);
      if (index !== undefined && isObject(event.content)) {
        next = put(message, index, { ...event.content, type: "image" });
      }
      break;
    }
    case "toolcall_end": {
      const index = slot(event.contentIndex);
      if (index !== undefined && isObject(event.toolCall)) {
        next = put(message, index, { ...event.toolCall, type: "toolCall" });
      }
      break;
    }
    case "done":
    case "error":
      break;
  }
  return next === message ? streaming : { ...streaming, message: next };
}
