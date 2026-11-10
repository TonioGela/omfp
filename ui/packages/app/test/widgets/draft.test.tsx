/** L4: the draft thread (UI-023) in `omfp:chat` and `omfp:header` over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DRAFT_MESSAGE, DRAFT_THREAD, ERRORS, SELECTED_THREAD, threadActions } from "../../src/sdk/index.ts";
import { ChatWidget } from "../../src/widgets/chat/index.ts";
import { HeaderWidget } from "../../src/widgets/header/index.ts";
import { repo, thread, workspace } from "../support/fixtures.ts";
import { renderWithHost } from "../support/host.tsx";

afterEach(cleanup);

function setup() {
  const test = renderWithHost(
    <>
      <HeaderWidget />
      <ChatWidget />
    </>,
  );
  test.client.connected();
  test.client.index({
    workspaces: [{ ...workspace("w1", [repo("w1", "fe", "main")]), name: "checkout" }],
    threads: [thread("t1", "older", "w1")],
  });
  test.flush();
  act(() => threadActions(test.host).openDraft("w1"));
  return { ...test, user: userEvent.setup() };
}

describe("draft thread", () => {
  // spec: UI-023
  test("it shows an empty conversation, a placeholder title with its workspace's chips, and a focused composer", async () => {
    setup();
    expect(screen.getByTestId("draft-thread-view").dataset.workspaceId).toBe("w1");
    expect(screen.getByTestId("draft-empty")).toBeDefined();
    const header = screen.getByTestId("thread-header");
    expect(header.dataset.draft).toBe("true");
    expect(within(header).getByTestId("thread-title").textContent).toBe("New thread");
    expect(
      within(header)
        .getAllByTestId("workspace-chip")
        .map((chip) => chip.textContent),
    ).toEqual(["⌂checkout", "fe:main"]);
    await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId("composer-input")));
  });

  // spec: UI-023
  test("the first message creates the daemon thread with it, then the real thread is selected", async () => {
    const { client, host, user } = setup();
    expect(client.callsTo("threads.create")).toHaveLength(0);
    await user.type(screen.getByTestId("composer-input"), "  fix the checkout {Enter}");
    expect(client.callsTo("threads.create").map((call) => call.params)).toEqual([
      { workspace: { id: "w1" }, prompt: "fix the checkout" },
    ]);
    // Pressing again while it is created sends nothing more.
    await user.click(screen.getByTestId("send"));
    expect(client.callsTo("threads.create")).toHaveLength(1);
    act(() => client.callsTo("threads.create")[0]?.resolve(thread("t9", "", "w1")));
    await waitFor(() => expect(host.window.get(SELECTED_THREAD)).toBe("t9"));
    expect(host.window.get(DRAFT_THREAD)).toBeUndefined();
    expect(host.window.get(DRAFT_MESSAGE).text).toBe("");
  });

  // spec: UI-023
  test("when the creation fails, the draft and its text stay and the error is reported", async () => {
    const { client, host, user } = setup();
    await user.type(screen.getByTestId("composer-input"), "fix it{Enter}");
    act(() => client.callsTo("threads.create")[0]?.reject(new Error("daemon down")));
    await waitFor(() => expect(host.window.get(ERRORS)).toEqual(["daemon down"]));
    expect(host.window.get(DRAFT_THREAD)).toEqual({ workspaceId: "w1" });
    expect(screen.getByTestId("composer-input")).toHaveProperty("value", "fix it");
    await waitFor(() => expect(screen.getByTestId("send").hasAttribute("disabled")).toBe(false));
  });

  // spec: UI-023
  test("Esc leaves the composer, discarding the draft only when it is empty", async () => {
    const { host, user } = setup();
    await user.type(screen.getByTestId("composer-input"), "keep{Escape}");
    expect(host.window.get(DRAFT_THREAD)).toEqual({ workspaceId: "w1" });
    await user.clear(screen.getByTestId("composer-input"));
    await user.keyboard("{Escape}");
    expect(host.window.get(DRAFT_THREAD)).toBeUndefined();
    expect(screen.queryByTestId("draft-thread-view")).toBeNull();
    expect(screen.getByTestId("no-thread")).toBeDefined();
  });
});
