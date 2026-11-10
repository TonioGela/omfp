import { describe, expect, test } from "bun:test";
import { type AgentMessage, emptyThreadState, type NoticeFrame, type ThreadState } from "@omfp/core";
import { items, messageView, streamingView } from "../../src/vm/transcript.ts";
import { fixture, fixtureUntil, isFrame } from "../widgets/chat/fixtures.ts";

function kinds(state: ThreadState): string[] {
  return items(state).map((item) =>
    item.kind.type === "message" ? messageView(state, item.kind.index).role : item.kind.type,
  );
}

function withMessage(message: AgentMessage): ThreadState {
  return { ...emptyThreadState(), transcript: [{ message_id: null, message }] };
}

describe("transcript items", () => {
  // spec: UI-040
  test("a text reply is a user message and an assistant text", () => {
    const state = fixture("text_reply");
    expect(kinds(state)).toEqual(["user", "assistant"]);
    const [block, ...rest] = messageView(state, 1).blocks;
    expect(rest).toEqual([]);
    expect(block?.type).toBe("text");
    expect(block?.type === "text" && block.text.length > 0).toBe(true);
  });

  // spec: UI-050
  test("tool results shown in their card are not listed again", () => {
    const state = fixture("bash_tool_call");
    expect(state.transcript.length).toBe(4);
    expect(kinds(state)).toEqual(["user", "assistant", "assistant"]);
  });

  // spec: UI-042
  test("the streaming reply is the last item with its text so far", () => {
    const state = fixtureUntil("text_reply", isFrame("message_update", "text_end"));
    expect(kinds(state).at(-1)).toBe("streaming");
    const blocks = streamingView(state)?.blocks ?? [];
    expect(blocks.length).toBe(1);
    expect(blocks[0]?.type === "text" && blocks[0].text.length > 0).toBe(true);
  });

  // spec: UI-043
  test("notices go after the messages that preceded them", () => {
    const frame: NoticeFrame = { type: "unknown", data: { type: "mystery" } };
    const state: ThreadState = {
      ...fixture("text_reply"),
      notices: [
        { position: 1, frame },
        { position: 2, frame },
        { position: 9, frame },
      ],
    };
    expect(kinds(state)).toEqual(["user", "notice", "assistant", "notice", "notice"]);
    expect(items(state).map((item) => item.key)).toEqual(["m0", "n0", "m1", "n1", "n2"]);
  });

  // spec: UI-041
  test("thinking is its own block unless it is narration", () => {
    const state = withMessage({
      role: "assistant",
      content: [
        { type: "thinking", thinking: "let me think" },
        { type: "thinking", thinking: "Here is the plan", narration: true },
        { type: "text", text: "done" },
      ],
    });
    expect(messageView(state, 0).blocks).toEqual([
      { type: "thinking", text: "let me think" },
      { type: "text", text: "Here is the plan" },
      { type: "text", text: "done" },
    ]);
  });

  // spec: UI-043
  test("other message kinds render as records with their JSON", () => {
    const view = messageView(withMessage({ role: "bashExecution", command: "ls", output: "a\nb", exitCode: 0 }), 0);
    expect([view.role, view.label]).toEqual(["record", "bash"]);
    const [block] = view.blocks;
    expect(block?.type === "record" && block.summary).toBe("ls");
    expect(block?.type === "record" && block.json.includes('"exitCode": 0')).toBe(true);
  });

  test("user messages keep their images and steering mark", () => {
    const view = messageView(
      withMessage({
        role: "user",
        steering: true,
        content: [
          { type: "text", text: "look" },
          { type: "image", data: "AAAA", mimeType: "image/jpeg" },
        ],
      }),
      0,
    );
    expect(view.steering).toBe(true);
    expect(view.blocks).toEqual([
      { type: "text", text: "look" },
      { type: "image", mime: "image/jpeg", data: "AAAA" },
    ]);
  });
});
