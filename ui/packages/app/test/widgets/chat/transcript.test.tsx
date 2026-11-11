/** L4: `omfp:transcript` over recorded fixtures. */

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import type { JsonObject, ThreadEvent } from "@omfp/core";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SELECTED_THREAD, scopesAt } from "../../../src/sdk/index.ts";
import { TranscriptWidget } from "../../../src/widgets/chat/index.ts";
import { renderWithHost } from "../../support/host.tsx";
import { fixtureEvents, isFrame } from "./fixtures.ts";
import { renderThread, THREAD } from "./harness.tsx";
import { fakeLayout } from "./layout.ts";

let restoreLayout: () => void = () => {};
beforeEach(() => {
  restoreLayout = fakeLayout();
});
afterEach(() => {
  cleanup();
  restoreLayout();
});

const transcript = () => screen.getByTestId("transcript");

/** A history of `count` user messages. */
function longHistory(count: number): ThreadEvent[] {
  return [
    {
      kind: "history",
      session: { session_id: "s", resumed: true },
      messages: Array.from({ length: count }, (_, index) => ({ role: "user" as const, content: `message ${index}` })),
    },
  ];
}

/** Sets the scroll metrics happy-dom does not compute and fires `scroll`. */
function scrollTo(element: HTMLElement, scrollTop: number, scrollHeight: number, clientHeight = 600): void {
  Object.defineProperty(element, "scrollHeight", { configurable: true, value: scrollHeight });
  Object.defineProperty(element, "clientHeight", { configurable: true, value: clientHeight });
  element.scrollTop = scrollTop;
  fireEvent.scroll(element);
}

describe("transcript", () => {
  // spec: UI-040, UI-094
  test("a text reply renders the user's text and the assistant's Markdown", () => {
    renderThread(<TranscriptWidget />, fixtureEvents("text_reply"));
    const messages = screen.getAllByTestId("message");
    expect(messages.map((message) => message.dataset.role)).toEqual(["user", "assistant"]);
    const [user, assistant] = messages;
    expect(user && within(user).getByTestId("plain-text").textContent).not.toBe("");
    const markdown = assistant && within(assistant).getByTestId("markdown");
    expect(markdown?.classList.contains("omfp-prose")).toBe(true);
    expect(markdown?.textContent).not.toBe("");
  });

  // spec: UI-040, UI-042, UI-147
  test("streaming deltas update the reply in place while it is marked streaming", () => {
    const events = fixtureEvents("text_reply");
    const start = events.findIndex(isFrame("message_update", "text_delta"));
    const { client, flush } = renderThread(<TranscriptWidget />, events.slice(0, start + 1));
    const streaming = () => screen.getAllByTestId("message").find((message) => message.dataset.streaming === "true");
    const first = streaming()?.textContent ?? "";
    expect(streaming()?.getAttribute("aria-busy")).toBe("true");
    const end = events.findIndex(isFrame("message_update", "text_end"));
    client.threadEvents(THREAD, ...events.slice(start + 1, end));
    flush();
    expect((streaming()?.textContent ?? "").length).toBeGreaterThan(first.length);
    client.threadEvents(THREAD, ...events.slice(end));
    flush();
    expect(streaming()).toBeUndefined();
  });

  // spec: UI-147, UI-191
  test("a streaming reply's Markdown is rendered in chunks that stay in place as it grows; the finished one whole", () => {
    const update = (assistantMessageEvent: JsonObject): ThreadEvent => ({
      kind: "omp",
      frame: { type: "message_update", messageId: "live", message: { role: "assistant" }, assistantMessageEvent },
    });
    const { client, flush } = renderThread(<TranscriptWidget />, [
      { kind: "omp", frame: { type: "message_start", messageId: "live", message: { role: "assistant", content: [] } } },
      update({ type: "text_start", contentIndex: 0, block: { type: "text", text: "" } }),
      update({ type: "text_delta", contentIndex: 0, delta: "First paragraph.\n\nSecond" }),
    ]);
    const chunks = () => [...screen.getByTestId("markdown").querySelectorAll(":scope > .omfp-markdown")];
    const [first, second] = chunks();
    expect([first?.textContent, second?.textContent]).toEqual(["First paragraph.", "Second"]);
    client.threadEvents(THREAD, update({ type: "text_delta", contentIndex: 0, delta: " half." }));
    flush();
    expect(chunks()[0]).toBe(first);
    expect(chunks()[1]?.textContent).toBe("Second half.");
    const text = "First paragraph.\n\nSecond half.";
    client.threadEvents(THREAD, update({ type: "text_end", contentIndex: 0, content: text, block: { type: "text" } }), {
      kind: "omp",
      frame: {
        type: "message_end",
        messageId: "live",
        message: { role: "assistant", content: [{ type: "text", text }] },
      },
    });
    flush();
    const markdown = screen.getByTestId("markdown");
    expect([markdown.classList.contains("omfp-markdown"), markdown.querySelectorAll("p").length]).toEqual([true, 2]);
  });

  // spec: UI-041, UI-171
  test("thinking is collapsed until expanded", async () => {
    const thinking: ThreadEvent = {
      kind: "history",
      session: { session_id: "s", resumed: true },
      messages: [{ role: "assistant", content: [{ type: "thinking", thinking: "secret plan" }] }],
    };
    renderThread(<TranscriptWidget />, [thinking]);
    const block = screen.getByTestId("thinking");
    expect(block.dataset.expanded).toBe("false");
    expect(within(block).queryByText("secret plan")).toBeNull();
    const head = within(block).getByTestId("thinking-head");
    expect(head.textContent).toBe("▸Thinking");
    await userEvent.click(head);
    expect(block.dataset.expanded).toBe("true");
    expect(head.textContent).toBe("▾Thinking");
    expect(within(block).getByText("secret plan").closest("[hidden]")).toBeNull();
  });

  // spec: UI-050
  test("a tool without a specialized card renders through the fallback card, arguments and result on demand", async () => {
    const call: ThreadEvent = {
      kind: "history",
      session: { session_id: "s", resumed: true },
      messages: [
        { role: "assistant", content: [{ type: "toolCall", id: "c1", name: "eval", arguments: { code: "1 + 1" } }] },
        { role: "toolResult", toolCallId: "c1", toolName: "eval", content: [{ type: "text", text: "2" }] },
      ],
    };
    renderThread(<TranscriptWidget />, [call]);
    const card = screen.getByTestId("tool-card");
    expect([card.dataset.tool, card.dataset.view, card.dataset.status]).toEqual(["eval", "generic", "succeeded"]);
    expect(within(card).getByTestId("tool-summary").textContent).toBe("code: 1 + 1");
    expect(within(card).queryByTestId("tool-result")).toBeNull();
    await userEvent.click(within(card).getByTestId("toggle"));
    expect(within(card).getByTestId("tool-args").textContent).toContain('"code"');
    expect(within(card).getByTestId("tool-result").textContent).toBe("2");
  });

  // spec: UI-050, UI-052
  test("a failed tool is distinct and expanded", () => {
    const failed: ThreadEvent = {
      kind: "history",
      session: { session_id: "s", resumed: true },
      messages: [
        { role: "assistant", content: [{ type: "toolCall", id: "c1", name: "read", arguments: { path: "x" } }] },
        { role: "toolResult", toolCallId: "c1", isError: true, content: [{ type: "text", text: "ENOENT" }] },
      ],
    };
    renderThread(<TranscriptWidget />, [failed]);
    const card = screen.getByTestId("tool-card");
    expect(card.dataset.status).toBe("failed");
    expect(card.querySelector(".omfp-tool")?.getAttribute("data-status")).toBe("failed");
    expect(within(card).getByTestId("tool-error").textContent).toBe("ENOENT");
  });

  // spec: UI-043
  test("notices are inline rows; unknown kinds expand to their JSON", async () => {
    const events: ThreadEvent[] = [
      ...fixtureEvents("text_reply"),
      { kind: "unknown", raw: { type: "irc_card", from: "advisor" } },
    ];
    renderThread(<TranscriptWidget />, events);
    const notice = screen.getAllByTestId("notice").find((row) => row.dataset.kind === "unknown");
    expect(notice).toBeDefined();
    if (notice !== undefined) {
      const json = within(notice).getByTestId("notice-json");
      expect(json.closest("[hidden]")).not.toBeNull();
      await userEvent.click(within(notice).getByTestId("toggle"));
      expect(json.closest("[hidden]")).toBeNull();
      expect(json.textContent).toContain('"irc_card"');
    }
  });

  // spec: UI-043
  test("omp's startup records are collapsed system rows; a record with nothing to summarize has no disclosure", async () => {
    const startup: ThreadEvent[] = [
      {
        kind: "history",
        session: { session_id: "s", resumed: true },
        messages: [
          {
            role: "custom",
            customType: "xdev-mount-notice",
            content:
              "<system-notice>\nxd:// device inventory changed.\n- xd://ast_edit — Structural edits\n</system-notice>",
            display: false,
            timestamp: 1,
          },
          { role: "custom", customType: "marker", content: "", display: true, timestamp: 2 },
        ],
      },
      { kind: "omp", frame: { type: "notice", level: "info", message: "xd://: mounted ast_edit", source: "xdev" } },
    ];
    renderThread(<TranscriptWidget />, startup);
    const rows = screen.getAllByTestId("system-record");
    expect(rows.map((row) => within(row).getByTestId("system-label").textContent)).toEqual(["system", "system"]);
    expect(rows.map((row) => within(row).getByTestId("system-summary").textContent)).toEqual([
      "xd:// device inventory changed.",
      "xd://: mounted ast_edit",
    ]);
    const [inventory, mounted] = rows;
    if (inventory === undefined || mounted === undefined) {
      throw new Error("two system rows expected");
    }
    expect(screen.queryAllByTestId("notice").length).toBe(0);
    const full = within(inventory).getByTestId("system-text");
    expect(full.closest("[hidden]")).not.toBeNull();
    await userEvent.click(within(inventory).getByTestId("toggle"));
    expect(full.closest("[hidden]")).toBeNull();
    expect(full.textContent).toBe("xd:// device inventory changed.\n- xd://ast_edit — Structural edits");
    expect(within(mounted).queryByTestId("toggle")).toBeNull();
    const [record, ...others] = screen.getAllByTestId("message");
    expect([record?.dataset.role, others.length]).toEqual(["record", 0]);
    expect(record && within(record).queryByTestId("toggle")).toBeNull();
  });

  // spec: UI-042
  test("a long transcript keeps only the items around the viewport in the DOM", () => {
    renderThread(<TranscriptWidget />, longHistory(500));
    const rendered = screen.getAllByTestId("transcript-item").length;
    expect(rendered).toBeGreaterThan(0);
    expect(rendered).toBeLessThan(30);
  });

  // spec: UI-042
  test("only the user unpins: wheel up and scroll-up keys, not layout scrolls; the bottom pins again", () => {
    renderThread(<TranscriptWidget />, longHistory(50));
    const element = transcript();
    expect(element.dataset.pinned).toBe("true");
    scrollTo(element, 1000, 5000);
    expect(element.dataset.pinned).toBe("true");
    fireEvent.wheel(element, { deltaY: -100 });
    expect(element.dataset.pinned).toBe("false");
    scrollTo(element, 4400, 5000);
    expect(element.dataset.pinned).toBe("true");
    fireEvent.keyDown(element, { key: "ArrowUp" });
    expect(element.dataset.pinned).toBe("false");
    scrollTo(element, 4400, 5000);
    fireEvent.pointerDown(element);
    scrollTo(element, 1000, 5000);
    expect(element.dataset.pinned).toBe("false");
    fireEvent.pointerUp(element);
  });

  // spec: UI-042
  test("switching threads and back returns to where the user had scrolled up to", () => {
    const { client, flush, host } = renderThread(<TranscriptWidget />, longHistory(50));
    fireEvent.wheel(transcript(), { deltaY: -100 });
    scrollTo(transcript(), 400, 5000);
    act(() => host.window.set(SELECTED_THREAD, "02T"));
    client.thread("02T", longHistory(3));
    flush();
    expect(transcript().dataset.threadId).toBe("02T");
    act(() => host.window.set(SELECTED_THREAD, THREAD));
    client.thread(THREAD, longHistory(50));
    flush();
    expect(transcript().scrollTop).toBe(400);
    expect(transcript().dataset.pinned).toBe("false");
  });

  // spec: UI-042, UI-095
  test("the transcript scrolls itself on PageUp/PageDown/Home/End", () => {
    renderThread(<TranscriptWidget />, longHistory(50));
    const element = transcript();
    scrollTo(element, 4400, 5000);
    // happy-dom does not clamp: pinned, the transcript set it to the full height.
    const bottom = element.scrollTop;
    fireEvent.keyDown(element, { key: "PageUp" });
    expect(element.scrollTop).toBe(bottom - 568);
    expect(element.dataset.pinned).toBe("false");
    fireEvent.keyDown(element, { key: "PageDown" });
    expect(element.scrollTop).toBe(4400);
    // The browser reports the scroll; back at the bottom, the view is pinned again.
    fireEvent.scroll(element);
    expect(element.dataset.pinned).toBe("true");
    fireEvent.keyDown(element, { key: "Home" });
    expect(element.scrollTop).toBe(0);
    expect(element.dataset.pinned).toBe("false");
    fireEvent.keyDown(element, { key: "End" });
    expect(element.scrollTop).toBeGreaterThanOrEqual(4400);
    expect(element.dataset.pinned).toBe("true");
  });

  // spec: UI-095
  test("transcript-scope nav commands move between messages", () => {
    const { host } = renderThread(<TranscriptWidget />, longHistory(5));
    const element = transcript();
    act(() => element.focus());
    const scopes = scopesAt(document.activeElement);
    expect(scopes).toEqual(["transcript"]);
    const run = (id: string) => act(() => void host.commands.run(id, [], scopesAt(document.activeElement)));
    const current = () => document.activeElement?.getAttribute("data-index");
    run("nav.prev");
    expect(current()).toBe("4");
    run("nav.prev");
    expect(current()).toBe("3");
    expect(element.dataset.pinned).toBe("false");
    run("nav.top");
    expect(current()).toBe("0");
    run("nav.next");
    expect(current()).toBe("1");
    run("nav.bottom");
    expect(current()).toBe("4");
    expect(element.dataset.pinned).toBe("true");
  });

  // spec: UI-092
  test("the agent starting and finishing is announced", () => {
    const events = fixtureEvents("text_reply");
    const busyAt = events.findIndex(isFrame("agent_start"));
    const { client, flush, host } = renderThread(<TranscriptWidget />, events.slice(0, busyAt));
    client.threadEvents(THREAD, ...events.slice(busyAt, busyAt + 1));
    flush();
    expect(host.announcer.getSnapshot().message).toBe("Agent working");
    client.threadEvents(THREAD, ...events.slice(busyAt + 1));
    flush();
    expect(host.announcer.getSnapshot().message).toBe("Agent finished");
  });

  test("placeholders while no thread is selected or its snapshot is missing", () => {
    const { host } = renderWithHost(<TranscriptWidget />);
    expect(screen.getByTestId("no-thread")).toBeDefined();
    act(() => host.window.set(SELECTED_THREAD, THREAD));
    expect(screen.getByTestId("thread-loading").dataset.threadId).toBe(THREAD);
  });
});
