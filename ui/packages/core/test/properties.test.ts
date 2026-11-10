/**
 * Property tests of the reducer (TST-011); port of `crates/omfp-core/tests/properties.rs` with a seeded PRNG instead
 * of proptest, so every run checks the same cases. The snapshot property runs exhaustively in `fixtures.test.ts`.
 */

import { describe, expect, test } from "bun:test";
import {
  emptyThreadState,
  type JsonObject,
  type JsonValue,
  type QueueState,
  reduce,
  type ThreadEvent,
  type ThreadState,
} from "../src/index.ts";
import { canonical, deepFreeze, event, FIXTURE_DIR, fixture, fixtureJson, jsonlNames, omp } from "./helpers.ts";

const CASES = 256;

/** Deterministic PRNG (mulberry32). */
class Random {
  private seed: number;

  constructor(seed: number) {
    this.seed = seed >>> 0;
  }

  /** Uniform in `[0, 1)`. */
  next(): number {
    this.seed = (this.seed + 0x6d2b79f5) >>> 0;
    let t = this.seed;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  }

  /** Uniform integer in `[0, bound)`. */
  below(bound: number): number {
    return Math.floor(this.next() * bound);
  }

  /** Uniform integer in `[min, max]`. */
  between(min: number, max: number): number {
    return min + this.below(max - min + 1);
  }

  pick<T>(items: readonly T[]): T {
    const item = items[this.below(items.length)];
    if (item === undefined) {
      throw new Error("pick from an empty list");
    }
    return item;
  }

  /** A string of `min..=max` characters (code points) from `alphabet`. */
  string(alphabet: readonly string[], min: number, max: number): string {
    return Array.from({ length: this.between(min, max) }, () => this.pick(alphabet)).join("");
  }
}

/** Printable characters, multi-byte and astral ones and JSON-significant ones included (proptest's `\PC`). */
const PRINTABLE = [..."abcXYZ019 .,:;!?'\"\\/{}[]<>éßñ中文한😀🦀\u00a0"];
const LOWER = [..."abcdefghijklmnopqrstuvwxyz"];
const LOWER_DIGIT_UNDERSCORE = [...LOWER, ..."0123456789_"];
const BASE64 = [..."ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"];

/** One generated content block of an assistant message. */
type Block =
  | { kind: "text"; text: string }
  | { kind: "thinking"; thinking: string; signature: string | undefined }
  | { kind: "toolCall"; id: string; name: string; arguments: Record<string, string> };

function randomBlock(random: Random): Block {
  switch (random.below(3)) {
    case 0:
      return { kind: "text", text: random.string(PRINTABLE, 0, 40) };
    case 1:
      return {
        kind: "thinking",
        thinking: random.string(PRINTABLE, 0, 40),
        signature: random.below(2) === 0 ? undefined : random.string(BASE64, 1, 12),
      };
    default: {
      const entries = Array.from({ length: random.below(4) }, (): [string, string] => [
        random.string(LOWER, 1, 6),
        random.string(PRINTABLE, 0, 20),
      ]);
      // A BTreeMap: sorted, distinct keys.
      entries.sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0));
      return {
        kind: "toolCall",
        id: random.string(LOWER_DIGIT_UNDERSCORE, 1, 10),
        name: random.string([...LOWER, "_"], 1, 8),
        arguments: Object.fromEntries(entries),
      };
    }
  }
}

function blockJson(block: Block): JsonObject {
  switch (block.kind) {
    case "text":
      return { type: "text", text: block.text };
    case "thinking":
      return block.signature === undefined
        ? { type: "thinking", thinking: block.thinking }
        : { type: "thinking", thinking: block.thinking, thinkingSignature: block.signature };
    case "toolCall":
      return { type: "toolCall", id: block.id, name: block.name, arguments: block.arguments };
  }
}

/** The text the deltas of this block carry, in order. */
function streamed(block: Block): string {
  switch (block.kind) {
    case "text":
      return block.text;
    case "thinking":
      return block.thinking;
    case "toolCall":
      return JSON.stringify(block.arguments);
  }
}

/** Splits `text` into pieces at up to `cuts` random code-point boundaries. */
function split(random: Random, text: string, cuts: number): string[] {
  const chars = [...text];
  const points = new Set([0, chars.length]);
  for (let cut = 0; cut < cuts; cut += 1) {
    points.add(random.below(chars.length + 1));
  }
  const sorted = [...points].sort((left, right) => left - right);
  return sorted.slice(1).map((end, at) => chars.slice(sorted[at], end).join(""));
}

function update(assistantEvent: JsonObject): ThreadEvent {
  return omp({
    type: "message_update",
    messageId: "msg-1",
    message: { role: "assistant" },
    assistantMessageEvent: assistantEvent,
  });
}

function startEvent(index: number, block: Block): ThreadEvent {
  switch (block.kind) {
    case "text":
      return update({ type: "text_start", contentIndex: index, block: { type: "text", text: "" } });
    case "thinking":
      return update({ type: "thinking_start", contentIndex: index, block: { type: "thinking", thinking: "" } });
    case "toolCall":
      return update({
        type: "toolcall_start",
        contentIndex: index,
        block: { type: "toolCall", id: block.id, name: block.name, arguments: {} },
      });
  }
}

function deltaEvent(index: number, block: Block, delta: string): ThreadEvent {
  const type = { text: "text_delta", thinking: "thinking_delta", toolCall: "toolcall_delta" }[block.kind];
  return update({ type, contentIndex: index, delta });
}

function endEvent(index: number, block: Block): ThreadEvent {
  switch (block.kind) {
    case "text":
      return update({ type: "text_end", contentIndex: index, content: block.text, block: { type: "text" } });
    case "thinking": {
      const shell: JsonObject =
        block.signature === undefined ? { type: "thinking" } : { type: "thinking", thinkingSignature: block.signature };
      return update({ type: "thinking_end", contentIndex: index, content: block.thinking, block: shell });
    }
    case "toolCall":
      return update({ type: "toolcall_end", contentIndex: index, toolCall: blockJson(block) });
  }
}

/** The streamed string of the rebuilt block at `index`: text, thinking, or a tool call's argument JSON so far. */
function rebuilt(state: ThreadState, index: number): string | undefined {
  const block = state.streaming?.message.content?.[index];
  switch (block?.type) {
    case "text":
      return block.text;
    case "thinking":
      return block.thinking;
    case "toolCall":
      return block.partialArgs;
    default:
      return undefined;
  }
}

/** Events that must not touch the queue, mixed between `queue_update`s. */
const OTHER_EVENTS: ThreadEvent[] = [
  omp({ type: "agent_start" }),
  omp({ type: "session_settled" }),
  omp({ type: "agent_end", messages: [] }),
  omp({ type: "message_end", messageId: "m", message: { role: "user", content: "hi" } }),
  omp({ type: "extension_ui_request", id: "s", method: "select", title: "t", options: ["a"] }),
  event({ kind: "process", state: { state: "running" } }),
  event({ kind: "process", state: { state: "failed", message: "exit" } }),
  event({ kind: "process", state: { state: "stopped" } }),
  event({ kind: "history", session: { session_id: "s", resumed: true }, messages: [] }),
  event({ kind: "unknown", raw: { type: "queue_update" } }),
];

function randomJson(random: Random, depth: number): JsonValue {
  switch (random.below(depth > 2 ? 5 : 7)) {
    case 0:
      return null;
    case 1:
      return random.below(2) === 0;
    case 2:
      return random.pick([0, 1, -1, 2.5, 1e300, 3]);
    case 3:
      return random.string(PRINTABLE, 0, 6);
    case 4:
      return random.pick(["text", "assistant", "toolCall", "select", "running", "__proto__", "constructor"]);
    case 5:
      return Array.from({ length: random.below(3) }, () => randomJson(random, depth + 1));
    default:
      return Object.fromEntries(
        Array.from({ length: random.below(4) }, () => [
          random.pick(["type", "id", "role", "content", "message", "contentIndex", "__proto__", "x"]),
          randomJson(random, depth + 1),
        ]),
      );
  }
}

/** `value` with one random member (at any depth) replaced by random JSON. */
function mutate(random: Random, value: JsonValue, depth: number): JsonValue {
  if (value === null || typeof value !== "object" || random.below(4) === 0) {
    return depth === 0 ? value : randomJson(random, depth);
  }
  if (Array.isArray(value)) {
    if (value.length === 0) {
      return [randomJson(random, depth + 1)];
    }
    const at = random.below(value.length);
    return value.map((item, index) => (index === at ? mutate(random, item, depth + 1) : item));
  }
  const keys = Object.keys(value);
  const key = keys.length === 0 || random.below(5) === 0 ? random.pick(["type", "kind", "x"]) : random.pick(keys);
  const member = Object.hasOwn(value, key) ? (value[key] ?? null) : null;
  return { ...value, [key]: mutate(random, member, depth + 1) };
}

function allFixtureEvents(): ThreadEvent[] {
  return jsonlNames(FIXTURE_DIR).flatMap((name) => fixture(name));
}

describe("properties", () => {
  // spec: TST-011
  test("delta updates rebuild the message_end content", () => {
    for (let seed = 0; seed < CASES; seed += 1) {
      const random = new Random(seed);
      const blocks = Array.from({ length: random.below(5) }, () => randomBlock(random));
      const content = blocks.map(blockJson);
      const finalMessage: JsonObject = {
        role: "assistant",
        content,
        stopReason: "stop",
        responseId: "r-1",
        timestamp: 0,
      };
      // omp snapshots `message_start` when it handles it, so it may already hold content the deltas deliver again.
      const startContent = random.below(2) === 0 ? content : [];
      let state = reduce(
        emptyThreadState(),
        omp({
          type: "message_start",
          messageId: "msg-1",
          message: { role: "assistant", content: startContent, timestamp: 0 },
        }),
      );

      for (const [index, block] of blocks.entries()) {
        state = reduce(state, startEvent(index, block));
        expect(rebuilt(state, index)).toBe("");
        const text = streamed(block);
        for (const piece of split(random, text, random.below(6))) {
          state = reduce(state, deltaEvent(index, block, piece));
        }
        expect(rebuilt(state, index)).toBe(text);
        state = reduce(state, endEvent(index, block));
      }

      expect(state.streaming?.message_id).toBe("msg-1");
      expect(canonical(state.streaming?.message.content)).toBe(canonical(content));

      state = reduce(state, omp({ type: "message_end", messageId: "msg-1", message: finalMessage }));
      expect(state.streaming).toBeNull();
      expect(state.transcript.length).toBe(1);
      expect(canonical(state.transcript[0]?.message)).toBe(canonical(finalMessage));
    }
  });

  // spec: TST-011
  test("the queue always equals the last queue_update", () => {
    for (let seed = 0; seed < CASES; seed += 1) {
      const random = new Random(seed);
      let state = emptyThreadState();
      let last: QueueState = { steering: [], follow_up: [], live_steered: 0 };
      for (let step = random.below(40); step > 0; step -= 1) {
        if (random.below(2) === 0) {
          const queue: QueueState = {
            steering: Array.from({ length: random.below(4) }, () => random.string(PRINTABLE, 0, 12)),
            follow_up: Array.from({ length: random.below(4) }, () => random.string(PRINTABLE, 0, 12)),
            live_steered: random.below(3),
          };
          state = reduce(
            state,
            omp({
              type: "queue_update",
              steering: queue.steering,
              followUp: queue.follow_up,
              liveSteered: queue.live_steered,
            }),
          );
          last = queue;
        } else {
          state = reduce(state, random.pick(OTHER_EVENTS));
        }
        expect(state.queue).toEqual(last);
      }
    }
  });

  // spec: ARC-006, TST-010
  test("any order of recorded events is total and the state survives a JSON round trip", () => {
    const pool = allFixtureEvents().map((item) => deepFreeze(item));
    for (let seed = 0; seed < 64; seed += 1) {
      const random = new Random(seed);
      let state = emptyThreadState();
      for (let step = random.below(200); step > 0; step -= 1) {
        state = reduce(deepFreeze(state), random.pick(pool));
      }
      const restored: unknown = JSON.parse(JSON.stringify(state));
      expect(restored).toEqual(state);
    }
  });

  // spec: ARC-006, TST-010
  test("reduce never throws on corrupted events and never mutates its inputs", () => {
    const pool = jsonlNames(FIXTURE_DIR).flatMap((name) => fixtureJson(name));
    for (let seed = 0; seed < CASES; seed += 1) {
      const random = new Random(seed);
      let state = emptyThreadState();
      for (let step = 0; step < 40; step += 1) {
        const source = random.pick(pool);
        const corrupted = deepFreeze(random.below(8) === 0 ? randomJson(random, 0) : mutate(random, source, 0));
        state = reduce(deepFreeze(state), event(corrupted));
      }
      const restored: unknown = JSON.parse(JSON.stringify(state));
      expect(restored).toEqual(state);
    }
  });
});
