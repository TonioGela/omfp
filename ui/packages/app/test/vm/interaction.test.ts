import { describe, expect, test } from "bun:test";
import type { ClosedInteraction, InteractionRequest } from "@omfp/client";
import type { AskUiRequest } from "@omfp/core";
import {
  type AskForm,
  cardKey,
  checkAsk,
  closedRows,
  emptyAskForm,
  formatCountdown,
  remainingMs,
  setCustom,
  timeoutMs,
  title,
  toggleOption,
} from "../../src/vm/interaction.ts";

const ASK: AskUiRequest = {
  id: "ask-1",
  timeout: 30000,
  questions: [
    { id: "color", question: "Which color?", multi: false, options: [{ label: "red" }, { label: "blue" }] },
    {
      id: "tools",
      question: "Which tools?",
      multi: true,
      options: [{ label: "bash" }, { label: "read" }, { label: "edit" }],
    },
  ],
};

function confirm(id: string): InteractionRequest {
  return { type: "extension_ui_request", method: "confirm", id, title: "Proceed?", message: "m" };
}

describe("ask form", () => {
  // spec: UI-071
  test("single select keeps one choice and free text replaces it", () => {
    let form: AskForm = emptyAskForm(ASK);
    form = toggleOption(form, ASK, 0, "red");
    form = toggleOption(form, ASK, 0, "blue");
    expect(form[0]?.selected).toEqual(["blue"]);
    form = setCustom(form, ASK, 0, "green");
    expect(form[0]).toEqual({ selected: [], custom: "green" });
    form = toggleOption(form, ASK, 0, "red");
    expect(form[0]).toEqual({ selected: ["red"], custom: "" });
  });

  // spec: UI-071
  test("multi select toggles and combines with free text in option order", () => {
    let form: AskForm = emptyAskForm(ASK);
    form = toggleOption(form, ASK, 0, "red");
    for (const label of ["edit", "bash", "read", "read"]) {
      form = toggleOption(form, ASK, 1, label);
    }
    form = setCustom(form, ASK, 1, "  grep ");
    const checked = checkAsk(form, ASK);
    expect(checked.ok).toBe(true);
    if (checked.ok) {
      expect(checked.answers[1]).toEqual({ id: "tools", selectedOptions: ["bash", "edit"], customInput: "grep" });
    }
  });

  // spec: UI-071
  test("every question needs an answer and omp's rules are enforced", () => {
    let form: AskForm = toggleOption(emptyAskForm(ASK), ASK, 1, "bash");
    expect(checkAsk(form, ASK)).toEqual({ ok: false, error: "Answer \u201cWhich color?\u201d" });
    form = setCustom(form, ASK, 0, "green");
    expect(checkAsk(form, ASK).ok).toBe(true);
    // A form edited into an invalid shape is refused by `validateAskAnswers`.
    const invalid: AskForm = [{ selected: ["red"], custom: "green" }, form[1] ?? { selected: [], custom: "" }];
    const checked = checkAsk(invalid, ASK);
    expect(checked.ok === false && checked.error.includes("single-select")).toBe(true);
  });
});

describe("interactions", () => {
  // spec: UI-073
  test("the countdown counts down from when the request was shown", () => {
    const request: InteractionRequest = { type: "extension_ui_request", method: "ask", ...ASK };
    const timeout = timeoutMs(request) ?? 0;
    expect(formatCountdown(remainingMs(timeout, 1_000, 1_000))).toBe("0:30");
    expect(formatCountdown(remainingMs(timeout, 1_000, 2_500))).toBe("0:29");
    expect(formatCountdown(remainingMs(timeout, 1_000, 40_000))).toBe("0:00");
    expect(formatCountdown(remainingMs(90_000, 0, 0))).toBe("1:30");
    expect(timeoutMs({ type: "extension_ui_request", method: "editor", id: "e", title: "Edit" })).toBeUndefined();
    expect(title(request)).toBe("Which color?");
  });

  // spec: UI-072
  test("requests closed elsewhere say who answered unless answered here", () => {
    const closed: ClosedInteraction[] = [
      { request: confirm("a"), resolution: { by: "client", client: "omfp/web" } },
      { request: confirm("b"), resolution: undefined },
      { request: confirm("c"), resolution: { by: "client", client: "omfp/web" } },
    ];
    const rows = closedRows(
      closed,
      (id) => id === "c",
      () => false,
    );
    expect(rows.map((row) => [row.id, row.text])).toEqual([
      ["a", "Answered elsewhere by omfp/web"],
      ["b", "Closed by omp (timed out or cancelled)"],
    ]);
    expect(rows[0]?.title).toBe("Proceed?");
    expect(
      closedRows(
        closed,
        () => false,
        (id) => id !== "b",
      ).length,
    ).toBe(1);
  });

  // spec: UI-070
  test("cards are keyed by request id and by position only without one", () => {
    expect([cardKey(confirm("c-7"), 0), cardKey(confirm("c-7"), 3)]).toEqual(["id-c-7", "id-c-7"]);
    expect(cardKey({ type: "extension_ui_request", method: "notify", id: "n", message: "hi" }, 2)).toBe("at-2");
  });
});
