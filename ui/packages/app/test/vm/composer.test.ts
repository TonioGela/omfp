import { describe, expect, test } from "bun:test";
import type { AvailableSlashCommand } from "@omfp/core";
import {
  alternative,
  base64,
  behavior,
  canAbort,
  commandsOf,
  complete,
  imageAttachment,
  isBusy,
  keyAction,
  type Modifiers,
  restoreDraft,
  restoredTexts,
  restoreFailedPrompt,
  slashSuggestions,
} from "../../src/vm/composer.ts";
import { fixture, fixtureUntil, isFrame } from "../widgets/chat/fixtures.ts";

const COMMANDS: AvailableSlashCommand[] = [
  { name: "compact", source: "builtin", description: "Compact the context" },
  { name: "model", source: "builtin", aliases: ["m"], input: { hint: "<model>" } },
  { name: "mcp", source: "builtin" },
  { name: "copy", source: "builtin", aliases: ["cp"] },
];

const PLAIN: Modifiers = { shift: false, alt: false, ctrl: false, meta: false };
const IDLE = { busy: false, abortable: false, hasSuggestions: false };
const BUSY = { busy: true, abortable: true, hasSuggestions: false };

const names = (draft: string) => slashSuggestions(draft, COMMANDS).map((suggestion) => suggestion.name);

describe("composer", () => {
  // spec: UI-061
  test("busy threads queue follow-ups unless the modifier steers", () => {
    const busy = fixtureUntil("follow_up_then_abort", isFrame("queue_update"));
    expect(isBusy(busy)).toBe(true);
    expect(isBusy(fixture("text_reply"))).toBe(false);
    const alt = { ...PLAIN, alt: true };
    expect(keyAction("Enter", PLAIN, BUSY)).toEqual({ type: "send", mode: "follow-up" });
    expect(keyAction("Enter", alt, BUSY)).toEqual({ type: "send", mode: "steer" });
    expect(keyAction("Enter", { ...PLAIN, meta: true }, BUSY)).toEqual({ type: "send", mode: "steer" });
    expect(keyAction("Enter", alt, IDLE)).toEqual({ type: "send", mode: "prompt" });
    expect(behavior("follow-up")).toBe("followUp");
    expect(behavior("steer")).toBe("steer");
    expect(behavior("prompt")).toBeUndefined();
    expect(alternative("follow-up")).toBe("steer");
    expect(alternative("prompt")).toBeUndefined();
  });

  // spec: UI-060
  test("shift+enter is a newline and tab completes only with suggestions", () => {
    expect(keyAction("Enter", { ...PLAIN, shift: true }, IDLE)).toEqual({ type: "none" });
    expect(keyAction("Tab", PLAIN, { ...IDLE, hasSuggestions: true })).toEqual({ type: "complete" });
    expect(keyAction("Tab", PLAIN, IDLE)).toEqual({ type: "none" });
  });

  // spec: UI-063, UI-095
  test("escape aborts while busy or queued, else leaves the composer", () => {
    expect(keyAction("Escape", PLAIN, BUSY)).toEqual({ type: "abort" });
    expect(keyAction("Escape", PLAIN, IDLE)).toEqual({ type: "leave" });
    const queued = fixtureUntil("follow_up_then_abort", isFrame("message_start"));
    expect(canAbort(fixture("text_reply"))).toBe(false);
    expect(canAbort({ ...fixture("text_reply"), queue: { steering: [], follow_up: ["x"], live_steered: 0 } })).toBe(
      true,
    );
    expect(canAbort(queued)).toBe(true);
  });

  // spec: UI-060
  test("slash completion matches names then aliases until a space", () => {
    expect(names("/c")).toEqual(["compact", "copy"]);
    expect(names("/m")).toEqual(["model", "mcp"]);
    expect(names("/cp")).toEqual(["copy"]);
    expect(names("/model gpt")).toEqual([]);
    expect(names("hello /m")).toEqual([]);
    const [model] = slashSuggestions("/mo", COMMANDS);
    expect(model?.hint).toBe("<model>");
    expect(model === undefined ? "" : complete(model)).toBe("/model ");
  });

  // spec: UI-060
  test("suggestions come from the thread's available commands", () => {
    const state = fixture("text_reply");
    const first = state.commands[0]?.name;
    expect(first).toBeDefined();
    expect(slashSuggestions(`/${first}`, state.commands)[0]?.name).toBe(first);
  });

  // spec: UI-060
  test("a thread.commands result gives the commands while the state has none", () => {
    const compact: AvailableSlashCommand = { name: "compact", source: "builtin" };
    expect(commandsOf({ commands: [compact, { nope: 1 }] })).toEqual([compact]);
    expect(commandsOf(undefined)).toEqual([]);
  });

  // spec: UI-063
  test("an abort puts the queued texts back before the draft", () => {
    expect(restoreDraft("still typing", ["steer me"], ["later please"])).toBe(
      "steer me\n\nlater please\n\nstill typing",
    );
    expect(restoreDraft("", [], ["later please"])).toBe("later please");
    expect(restoredTexts({ steering: [{ text: "a" }], followUp: [{ text: "b", images: [] }] })).toEqual({
      steering: ["a"],
      followUp: ["b"],
    });
    expect(restoredTexts(null)).toEqual({ steering: [], followUp: [] });
  });

  // spec: UI-064
  test("a failed send puts its text and images back before what was typed since", () => {
    const image = { type: "image" as const, data: "AA==", mimeType: "image/png" };
    expect(restoreFailedPrompt({ text: "more", images: [] }, "first", [image])).toEqual({
      text: "first\n\nmore",
      images: [image],
    });
  });

  // spec: UI-060
  test("only images become attachments encoded as base64", () => {
    const bytes = (text: string) => new TextEncoder().encode(text);
    expect(base64(bytes(""))).toBe("");
    expect(base64(bytes("f"))).toBe("Zg==");
    expect(base64(bytes("ab"))).toBe("YWI=");
    expect(base64(bytes("foobar"))).toBe("Zm9vYmFy");
    expect(imageAttachment("image/png", new Uint8Array([0x89, 0x50, 0x4e, 0x47]))).toEqual({
      type: "image",
      data: "iVBORw==",
      mimeType: "image/png",
    });
    expect(imageAttachment("text/plain", bytes("hi"))).toBeUndefined();
  });
});
