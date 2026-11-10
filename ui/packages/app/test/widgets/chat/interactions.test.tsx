/** L4: `omfp:interactions` (and inline in `omfp:chat`) over recorded fixtures. */

import { afterEach, describe, expect, test } from "bun:test";
import type { AskUiRequest, ThreadEvent } from "@omfp/core";
import { act, cleanup, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ChatWidget, InteractionsWidget } from "../../../src/widgets/chat/index.ts";
import { fixtureEvents } from "./fixtures.ts";
import { ManualClock, renderThread, THREAD } from "./harness.tsx";

afterEach(cleanup);

/** `approval_granted` up to (excluding) the daemon resolving the approval select, and that resolution. */
function approval(): { pending: ThreadEvent[]; resolved: ThreadEvent } {
  const events = fixtureEvents("approval_granted");
  const at = events.findIndex((event) => event.kind === "interaction_resolved");
  const resolved = events[at];
  if (resolved === undefined) {
    throw new Error("fixture without a resolution");
  }
  return { pending: events.slice(0, at), resolved };
}

const ASK: AskUiRequest = {
  id: "ask-1",
  timeout: 30000,
  questions: [
    { id: "db", question: "Which database?", multi: false, options: [{ label: "Postgres" }, { label: "SQLite" }] },
    {
      id: "features",
      question: "Which features?",
      multi: true,
      options: [{ label: "Auth" }, { label: "Billing" }, { label: "Search" }],
    },
  ],
};

function request(frame: Record<string, unknown>): ThreadEvent {
  return { kind: "omp", frame: { type: "extension_ui_request", ...frame } } as ThreadEvent;
}

const base = () => fixtureEvents("text_reply");

describe("interactions", () => {
  // spec: UI-070
  test("an approval select renders inline at the bottom of the transcript and takes the focus", async () => {
    const { client } = renderThread(<ChatWidget />, approval().pending);
    const card = screen.getByTestId("interaction");
    expect(card.dataset.kind).toBe("select");
    expect(card.closest('[data-testid="transcript"]')).not.toBeNull();
    const options = within(card).getAllByTestId("interaction-option");
    expect(options.map((option) => option.textContent)).toEqual(["Approve", "Approve bash for this session", "Deny"]);
    expect(document.activeElement === options[0]).toBe(true);
    await userEvent.click(options[0] as HTMLElement);
    expect(client.callsTo("interaction.respond").map((call) => call.params)).toEqual([
      { threadId: THREAD, interactionId: "<id>", response: { value: "Approve" } },
    ]);
    expect(card.getAttribute("aria-busy")).toBe("true");
  });

  // spec: UI-092
  test("a new request is announced", () => {
    const { pending } = approval();
    const { client, flush, host } = renderThread(<InteractionsWidget />, pending.slice(0, -1));
    client.threadEvents(THREAD, ...pending.slice(-1));
    flush();
    expect(host.announcer.getSnapshot().message).toBe("Request: Allow tool: bash\nCommand: echo approved-run");
  });

  // spec: UI-072
  test("a request answered by another client closes with who resolved it", async () => {
    const { pending, resolved } = approval();
    const { client, flush } = renderThread(<InteractionsWidget />, pending);
    client.threadEvents(THREAD, resolved);
    flush();
    expect(screen.queryByTestId("interaction")).toBeNull();
    const closed = screen.getByTestId("interaction-closed");
    expect(closed.textContent).toContain("Answered elsewhere by contract-test");
    await userEvent.click(within(closed).getByTestId("interaction-closed-dismiss"));
    expect(screen.queryByTestId("interaction-closed")).toBeNull();
  });

  // spec: UI-072
  test("a request answered here closes without an 'answered elsewhere' row; a refused answer can be retried", async () => {
    const { pending, resolved } = approval();
    const { client, flush } = renderThread(<InteractionsWidget />, pending);
    await userEvent.click(screen.getAllByTestId("interaction-option")[2] as HTMLElement);
    const [first] = client.callsTo("interaction.respond");
    await act(async () => first?.reject(new Error("bad_request")));
    expect(screen.getByTestId("interaction").getAttribute("aria-busy")).toBe("false");
    await userEvent.click(screen.getAllByTestId("interaction-option")[2] as HTMLElement);
    client.threadEvents(THREAD, resolved);
    flush();
    expect(screen.queryByTestId("interaction")).toBeNull();
    expect(screen.queryByTestId("interaction-closed")).toBeNull();
  });

  // spec: UI-071
  test("ask offers free text for every question and enforces single/multi rules before sending", async () => {
    const { client } = renderThread(<InteractionsWidget />, [...base(), request({ method: "ask", ...ASK })]);
    const form = screen.getByTestId("ask-form");
    const questions = within(form).getAllByTestId("ask-question");
    expect(questions.map((question) => question.dataset.multi)).toEqual(["false", "true"]);
    expect(within(form).getAllByTestId("ask-custom").length).toBe(2);
    const submit = within(form).getByTestId("interaction-submit");
    expect(submit.hasAttribute("disabled") || submit.getAttribute("aria-disabled") === "true").toBe(true);
    expect(within(form).getByTestId("ask-error").textContent).toBe("Answer \u201cWhich database?\u201d");

    const [single, multi] = questions as [HTMLElement, HTMLElement];
    await userEvent.click(within(single).getAllByTestId("ask-option")[0] as HTMLElement);
    await userEvent.click(within(single).getAllByTestId("ask-option")[1] as HTMLElement);
    await userEvent.click(within(multi).getAllByTestId("ask-option")[2] as HTMLElement);
    await userEvent.click(within(multi).getAllByTestId("ask-option")[0] as HTMLElement);
    await userEvent.type(within(multi).getByTestId("ask-custom"), "Audit");
    expect(within(form).queryByTestId("ask-error")).toBeNull();
    await userEvent.click(submit);
    expect(client.callsTo("interaction.respond")[0]?.params).toEqual({
      threadId: THREAD,
      interactionId: "ask-1",
      response: {
        answers: [
          { id: "db", selectedOptions: ["SQLite"] },
          { id: "features", selectedOptions: ["Auth", "Search"], customInput: "Audit" },
        ],
      },
    });
  });

  // spec: UI-071
  test("free text replaces the choice of a single-select question", async () => {
    renderThread(<InteractionsWidget />, [...base(), request({ method: "ask", ...ASK })]);
    const [single] = screen.getAllByTestId("ask-question") as [HTMLElement];
    const radio = within(single).getAllByTestId("ask-option")[0] as HTMLElement;
    await userEvent.click(radio);
    expect(radio.dataset.selected).toBe("true");
    await userEvent.type(within(single).getByTestId("ask-custom"), "MySQL");
    expect(radio.dataset.selected).toBeUndefined();
  });

  // spec: UI-073
  test("an announced timeout shows a countdown from when the request was shown", () => {
    const clock = new ManualClock(1_000);
    renderThread(<InteractionsWidget />, [...base(), request({ method: "ask", ...ASK })], clock);
    const countdown = screen.getByTestId("countdown");
    expect(countdown.textContent).toBe("0:30");
    act(() => clock.advance(1_500));
    expect(countdown.textContent).toBe("0:29");
    act(() => clock.advance(40_000));
    expect(countdown.textContent).toBe("0:00");
    expect(countdown.dataset.expired).toBe("true");
  });

  // spec: UI-070
  test("confirm, input and editor requests answer with their own shapes", async () => {
    const { client } = renderThread(<InteractionsWidget />, [
      ...base(),
      request({ method: "confirm", id: "c", title: "Proceed?", message: "Really?" }),
      request({ method: "input", id: "i", title: "Name?", placeholder: "name" }),
      request({ method: "editor", id: "e", title: "Edit", prefill: "draft" }),
    ]);
    expect(screen.queryByTestId("countdown")).toBeNull();
    expect(document.activeElement).toBe(screen.getByTestId("interaction-confirm"));
    await userEvent.click(screen.getByTestId("interaction-confirm"));
    await userEvent.type(screen.getByTestId("interaction-input"), "omfp{Enter}");
    const editor = screen.getByTestId("interaction-editor") as HTMLTextAreaElement;
    expect(editor.value).toBe("draft");
    await userEvent.type(editor, "!");
    const cards = screen.getAllByTestId("interaction");
    await userEvent.click(within(cards[2] as HTMLElement).getByTestId("interaction-submit"));
    expect(client.callsTo("interaction.respond").map((call) => call.params)).toEqual([
      { threadId: THREAD, interactionId: "c", response: { confirmed: true } },
      { threadId: THREAD, interactionId: "i", response: { value: "omfp" } },
      { threadId: THREAD, interactionId: "e", response: { value: "draft!" } },
    ]);
  });

  test("dismissing a request cancels it", async () => {
    const { client } = renderThread(<InteractionsWidget />, approval().pending);
    await userEvent.click(screen.getByTestId("interaction-dismiss"));
    expect(client.callsTo("interaction.respond")[0]?.params).toEqual({
      threadId: THREAD,
      interactionId: "<id>",
      response: { cancelled: true },
    });
  });
});
