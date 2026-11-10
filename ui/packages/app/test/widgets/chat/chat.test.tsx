/** L4: `omfp:chat` composes the transcript, the inline interactions, the queue and the composer. */

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { cleanup, screen, within } from "@testing-library/react";
import { ChatWidget } from "../../../src/widgets/chat/index.ts";
import { renderWithHost } from "../../support/host.tsx";
import { fixtureEvents } from "./fixtures.ts";
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

describe("chat", () => {
  // spec: UI-070, UI-062
  test("the selected thread's transcript with its requests inline, then queue chips and composer", () => {
    renderThread(<ChatWidget />, fixtureEvents("follow_up_then_abort").slice(0, 12));
    const thread = screen.getByTestId("thread");
    expect(thread.dataset.threadId).toBe(THREAD);
    const transcript = within(thread).getByTestId("transcript");
    expect(within(transcript).getAllByTestId("message").length).toBeGreaterThan(0);
    expect(within(thread).getByTestId("queue-chip").textContent).toContain("later please");
    expect(within(thread).getByTestId("composer").dataset.busy).toBe("true");
  });

  test("without a selected thread it says so", () => {
    renderWithHost(<ChatWidget />);
    expect(screen.getByTestId("no-thread")).toBeDefined();
  });
});
