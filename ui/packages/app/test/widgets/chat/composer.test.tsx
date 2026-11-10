/** L4: `omfp:composer` (queue chips and composer) over recorded fixtures. */

import { afterEach, describe, expect, test } from "bun:test";
import type { ThreadEvent } from "@omfp/core";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { draftKey, SELECTED_THREAD } from "../../../src/sdk/index.ts";
import { ChatWidget, ComposerWidget } from "../../../src/widgets/chat/index.ts";
import { eventsUntil, fixtureEvents, isFrame } from "./fixtures.ts";
import { renderThread, THREAD } from "./harness.tsx";

afterEach(cleanup);

const input = () => screen.getByTestId("composer-input") as HTMLTextAreaElement;

/** The thread right after the follow-up was queued: busy, one chip. */
function queuedEvents(): ThreadEvent[] {
  return fixtureEvents("follow_up_then_abort").slice(0, 12);
}

function busyEvents(): ThreadEvent[] {
  return eventsUntil("text_reply", isFrame("message_update", "text_end"));
}

describe("composer", () => {
  // spec: UI-060
  test("multi-line input: Shift+Enter is a newline, Enter sends and clears the draft", async () => {
    const { client } = renderThread(<ComposerWidget />, fixtureEvents("text_reply"));
    await userEvent.click(input());
    await userEvent.keyboard("first line{Shift>}{Enter}{/Shift}second line");
    expect(input().value).toBe("first line\nsecond line");
    expect(screen.getByTestId("send").dataset.mode).toBe("prompt");
    await userEvent.keyboard("{Enter}");
    expect(client.callsTo("thread.prompt").map((call) => call.params)).toEqual([
      { threadId: THREAD, text: "first line\nsecond line" },
    ]);
    expect(input().value).toBe("");
  });

  // spec: UI-064
  test("a prompt that fails to send comes back into the draft", async () => {
    const { client } = renderThread(<ComposerWidget />, fixtureEvents("text_reply"));
    await userEvent.click(input());
    await userEvent.keyboard("hello{Enter}");
    await userEvent.keyboard("more");
    await act(async () => client.callsTo("thread.prompt")[0]?.reject(new Error("offline")));
    expect(input().value).toBe("hello\n\nmore");
  });

  // spec: UI-060
  test("`/` completes from the thread's commands, kept fresh by available_commands_update", async () => {
    const { client, flush } = renderThread(<ComposerWidget />, fixtureEvents("text_reply"));
    await userEvent.click(input());
    await userEvent.keyboard("/secu");
    const menu = await screen.findByTestId("slash-suggestions");
    expect(within(menu).getAllByTestId("slash-suggestion")[0]?.textContent).toContain("/security");
    await userEvent.keyboard("{Tab}");
    expect(input().value).toBe("/security ");
    expect(screen.queryByTestId("slash-suggestions")).toBeNull();

    client.threadEvents(THREAD, {
      kind: "omp",
      frame: {
        type: "available_commands_update",
        commands: [{ name: "zebra", source: "custom", description: "Stripes" }],
      },
    });
    flush();
    await userEvent.clear(input());
    await userEvent.keyboard("/ze");
    const fresh = await screen.findByTestId("slash-suggestions");
    expect(
      within(fresh)
        .getAllByTestId("slash-suggestion")
        .map((item) => item.textContent),
    ).toEqual(["/zebra — Stripes"]);
  });

  // spec: UI-060
  test("a thread without announced commands asks thread.commands once", async () => {
    const { client } = renderThread(<ComposerWidget />, []);
    const [asked, ...again] = client.callsTo("thread.commands");
    expect([asked?.params, again.length]).toEqual([{ threadId: THREAD }, 0]);
    await act(async () => asked?.resolve({ commands: [{ name: "compact", source: "builtin" }] }));
    await userEvent.click(input());
    await userEvent.keyboard("/co");
    const menu = await screen.findByTestId("slash-suggestions");
    expect(
      within(menu)
        .getAllByTestId("slash-suggestion")
        .map((item) => item.textContent),
    ).toEqual(["/compact"]);
  });

  // spec: UI-060
  test("pasted and dropped images become attachments sent with the prompt", async () => {
    const { client } = renderThread(<ComposerWidget />, fixtureEvents("text_reply"));
    const png = new File([new Uint8Array([0x89, 0x50, 0x4e, 0x47])], "a.png", { type: "image/png" });
    const text = new File(["hi"], "a.txt", { type: "text/plain" });
    fireEvent.paste(input(), { clipboardData: { files: [png], getData: () => "" } });
    await waitFor(() => expect(screen.getAllByTestId("attachment").length).toBe(1));
    fireEvent.drop(screen.getByTestId("composer"), { dataTransfer: { files: [png, text] } });
    await waitFor(() => expect(screen.getAllByTestId("attachment").length).toBe(2));
    await userEvent.click(screen.getAllByTestId("attachment-remove")[1] as HTMLElement);
    expect(screen.getAllByTestId("attachment").length).toBe(1);
    await userEvent.click(screen.getByTestId("send"));
    expect(client.callsTo("thread.prompt")[0]?.params).toEqual({
      threadId: THREAD,
      text: "",
      images: [{ type: "image", data: "iVBORw==", mimeType: "image/png" }],
    });
  });

  // spec: UI-061
  test("while busy, sending queues a follow-up; the modifier steers; the button says which", async () => {
    const { client } = renderThread(<ComposerWidget />, busyEvents());
    const send = screen.getByTestId("send");
    expect([send.dataset.mode, send.textContent]).toEqual(["follow-up", "Queue follow-up"]);
    expect(screen.getByTestId("send-alternative").dataset.mode).toBe("steer");
    await userEvent.click(input());
    await userEvent.keyboard("later{Enter}");
    await userEvent.keyboard("{Alt>}");
    expect(screen.getByTestId("send").dataset.mode).toBe("steer");
    expect(screen.getByTestId("send").textContent).toBe("Steer now");
    await userEvent.keyboard("{/Alt}now{Alt>}{Enter}{/Alt}");
    expect(client.callsTo("thread.prompt").map((call) => call.params)).toEqual([
      { threadId: THREAD, text: "later", behavior: "followUp" },
      { threadId: THREAD, text: "now", behavior: "steer" },
    ]);
    expect(screen.getByTestId("send").dataset.mode).toBe("follow-up");
  });

  // spec: UI-063, UI-092
  test("Esc while busy aborts and puts the restored texts back into the draft", async () => {
    const { client, host } = renderThread(<ComposerWidget />, queuedEvents());
    await userEvent.click(input());
    await userEvent.keyboard("typing");
    await userEvent.keyboard("{Escape}");
    const [abort] = client.callsTo("thread.abort");
    expect(abort?.params).toEqual({ threadId: THREAD });
    await act(async () => abort?.resolve({ steering: [], followUp: [{ text: "later please" }] }));
    expect(input().value).toBe("later please\n\ntyping");
    expect(host.announcer.getSnapshot().message).toBe("Turn aborted");
    await userEvent.click(screen.getByTestId("abort"));
    expect(client.callsTo("thread.abort").length).toBe(2);
  });

  // spec: UI-095
  test("Esc while idle leaves the composer for the transcript; `composer.focus` comes back", async () => {
    const { host } = renderThread(<ChatWidget />, fixtureEvents("text_reply"));
    await userEvent.click(input());
    await userEvent.keyboard("{Escape}");
    expect(document.activeElement).toBe(screen.getByTestId("transcript"));
    act(() => void host.commands.run("composer.focus"));
    expect(document.activeElement).toBe(input());
  });

  // spec: UI-064
  test("drafts are kept per thread across thread switches", async () => {
    const { client, flush, host } = renderThread(<ComposerWidget />, fixtureEvents("text_reply"));
    await userEvent.click(input());
    await userEvent.keyboard("draft for one");
    act(() => host.window.set(SELECTED_THREAD, "02T"));
    client.thread("02T", fixtureEvents("text_reply"));
    flush();
    expect(input().value).toBe("");
    await userEvent.click(input());
    await userEvent.keyboard("other");
    act(() => host.window.set(SELECTED_THREAD, THREAD));
    client.thread(THREAD, fixtureEvents("text_reply"));
    flush();
    expect(input().value).toBe("draft for one");
    expect(host.window.get(draftKey("02T")).text).toBe("other");
  });
});

describe("queue chips", () => {
  // spec: UI-062
  test("chips come from queue_update only; remove waits for the daemon's answer", async () => {
    const { client, flush } = renderThread(<ComposerWidget />, queuedEvents());
    const chip = screen.getByTestId("queue-chip");
    expect([chip.dataset.queue, chip.getAttribute("aria-busy")]).toEqual(["follow-up", "false"]);
    await userEvent.click(within(chip).getByTestId("queue-remove"));
    const [remove] = client.callsTo("thread.queue.remove");
    expect(remove?.params).toEqual({ threadId: THREAD, message: "later please", queue: "followUp" });
    expect(screen.getByTestId("queue-chip").getAttribute("aria-busy")).toBe("true");
    await act(async () => remove?.resolve({ removed: true }));
    // Still shown until the next queue_update says otherwise.
    expect(screen.getByTestId("queue-chip").getAttribute("aria-busy")).toBe("false");
    client.threadEvents(THREAD, ...fixtureEvents("follow_up_then_abort").slice(12, 13));
    flush();
    expect(screen.queryByTestId("queue-chip")).toBeNull();
  });

  // spec: UI-062, UI-092
  test("a promote the daemon could not apply is announced and leaves the chip", async () => {
    const { client, host } = renderThread(<ComposerWidget />, queuedEvents());
    await userEvent.click(screen.getByTestId("queue-promote"));
    await act(async () => client.callsTo("thread.queue.promote")[0]?.resolve({ promoted: false }));
    expect(host.announcer.getSnapshot().message).toBe("\u201clater please\u201d is no longer queued");
    expect(screen.getByTestId("queue-chip").getAttribute("aria-busy")).toBe("false");
  });
});
