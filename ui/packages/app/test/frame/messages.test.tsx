/** L4: the window's messages, `:messages` (UI-521..UI-523). */

import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { sheetGroups } from "../../src/frame/commands-sheet.ts";
import { Frame } from "../../src/frame/frame.tsx";
import { messageEntries } from "../../src/frame/messages.tsx";
import {
  MESSAGES,
  NOTICES,
  reportError,
  reportSuccess,
  SELECTED_THREAD,
  type WidgetComponent,
} from "../../src/sdk/index.ts";
import { MESSAGES_KEPT } from "../../src/sdk/notify.ts";
import type { WidgetRegistry } from "../../src/view/resolve.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { ManualTimers, renderWithHost, testHost } from "../support/host.tsx";

afterEach(cleanup);

const stub =
  (name: string): WidgetComponent =>
  () => <p data-testid={`stub-${name}`}>{name}</p>;

const registry: WidgetRegistry = new Map<string, WidgetComponent>([
  ["omfp:header", stub("header")],
  ["omfp:chat", stub("chat")],
  ["omfp:workspace", stub("workspace")],
]);

/** 2026-11-13 14:03:12 UTC. */
const T0 = Date.UTC(2026, 10, 13, 14, 3, 12);

/** A focused frame on `t1`; `t2` is another thread of the workspace. */
function setup() {
  const noticeTimers = new ManualTimers();
  noticeTimers.time = T0;
  const base = testHost(undefined, {
    importer: () => new Promise<unknown>(() => {}),
    reportTimers: new ManualTimers(),
    noticeTimers,
  });
  base.client.answers.set("widgets.report", () => ({}));
  const test = renderWithHost(<Frame registry={registry} now={() => 0} hasFocus={() => true} />, base);
  test.client.connected();
  test.client.index({
    workspaces: [workspace("w1")],
    threads: [thread("t1", "checkout", "w1"), thread("t2", "flaky", "w1")],
  });
  act(() => test.host.window.set(SELECTED_THREAD, "t1"));
  test.flush();
  return { ...test, noticeTimers, user: userEvent.setup() };
}

function press(key: string): void {
  act(() => {
    fireEvent.keyDown(document.activeElement ?? document.body, { key });
  });
}

/** `[kind, title, text, dateTime]` per line of the open messages dialog. */
function listed(): string[][] {
  return within(screen.getByTestId("messages-dialog"))
    .queryAllByTestId("message")
    .map((line) => [
      line.dataset.tone ?? "",
      line.querySelector(".omfp-messages__kind")?.textContent ?? "",
      line.querySelector(".omfp-messages__title")?.textContent ?? "",
      line.querySelector(".omfp-messages__text")?.textContent ?? "",
      line.querySelector("time")?.getAttribute("datetime") ?? "",
    ]);
}

describe("the window's messages", () => {
  // spec: UI-521
  test("keep the notices and the toasts with their time, the last 100, oldest first", () => {
    const timers = new ManualTimers();
    const { host } = testHost(undefined, { noticeTimers: timers });
    timers.time = T0;
    host.notices.notify({ title: "flaky", body: "failed", threadId: "t2" }, "failed");
    timers.time = T0 + 1000;
    reportSuccess(host, "Copied to ./widgets/board");
    timers.time = T0 + 2000;
    reportError(host, new Error("disk full"));
    expect(host.window.get(MESSAGES).map(({ tone, title, text, at }) => [tone, title, text, at])).toEqual([
      ["failed", "flaky", "failed", T0],
      ["success", undefined, "Copied to ./widgets/board", T0 + 1000],
      ["error", undefined, "disk full", T0 + 2000],
    ]);
    // The notice goes from the mode line; the messages keep it.
    timers.run();
    expect([host.window.get(NOTICES).length, host.window.get(MESSAGES).length]).toEqual([0, 3]);
    for (let index = 0; index < MESSAGES_KEPT; index += 1) {
      reportSuccess(host, `done ${index}`);
    }
    const kept = host.window.get(MESSAGES);
    expect([kept.length, kept[0]?.text, kept.at(-1)?.text]).toEqual([MESSAGES_KEPT, "done 0", "done 99"]);
  });

  // spec: UI-522
  test("are listed newest first with their kind, tone and time", () => {
    const entries = messageEntries(
      [
        { id: 1, tone: "attention", title: "flaky", text: "needs you", at: T0 },
        { id: 2, tone: "success", text: "Folder path copied", at: T0 + 5000 },
      ],
      (at) => `t${at - T0}`,
    );
    expect(
      entries.map(({ tone, kind, time, dateTime, title, text }) => [tone, kind, time, dateTime, title, text]),
    ).toEqual([
      ["ok", "done", "t5000", "2026-11-13T14:03:17.000Z", undefined, "Folder path copied"],
      ["attention", "notice", "t0", "2026-11-13T14:03:12.000Z", "flaky", "needs you"],
    ]);
  });

  // spec: UI-522, UI-523
  test("a notice gone from the mode line is still there: the messages button opens them, Escape closes them", async () => {
    const { client, flush, host, noticeTimers, user } = setup();
    expect(screen.queryAllByTestId("mode-messages").length).toBe(0);
    client.indexEvents({ kind: "thread_upserted", thread: thread("t2", "flaky", "w1", { needsAttention: true }) });
    flush();
    expect(screen.getAllByTestId("mode-notice").length).toBe(1);
    act(() => noticeTimers.run());
    expect(screen.queryAllByTestId("mode-notice").length).toBe(0);
    noticeTimers.time = T0 + 60_000;
    act(() => reportSuccess(host, "Folder path copied"));
    await user.click(screen.getByTestId("mode-messages"));
    expect(listed()).toEqual([
      ["ok", "◆ done", "", "Folder path copied", "2026-11-13T14:04:12.000Z"],
      ["attention", "◆ notice", "flaky", "needs you", "2026-11-13T14:03:12.000Z"],
    ]);
    await user.keyboard("{Escape}");
    expect(screen.queryAllByTestId("messages-dialog").length).toBe(0);
    expect(document.activeElement?.getAttribute("data-testid")).toBe("mode-messages");
  });

  // spec: UI-522, UI-523
  test("`:messages`, `Space h e` and the phone's Commands sheet open them; Close closes them", async () => {
    const { host, user } = setup();
    expect(host.commands.resolve("messages")?.id).toBe("help.messages");
    press(":");
    await user.type(screen.getByTestId("command-line-input"), "messages{Enter}");
    expect(within(screen.getByTestId("messages-dialog")).getByText(/^No messages yet/).textContent).toContain(
      "No messages yet",
    );
    await user.click(screen.getByTestId("messages-close"));
    expect(screen.queryAllByTestId("messages-dialog").length).toBe(0);
    document.body.focus();
    press(" ");
    press("h");
    press("e");
    expect(screen.getAllByTestId("messages-dialog").length).toBe(1);
    const help = sheetGroups(host.commands.list(), [], "").find((group) => group.group === "help");
    expect(help?.commands.find((command) => command.id === "help.messages")?.name).toBe("messages");
  });
});
