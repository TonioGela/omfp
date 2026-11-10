/** Boundary tests of `ask` answer validation (DMN-054); port of `crates/omfp-core/tests/ask.rs`. */

import { describe, expect, test } from "bun:test";
import { type AskAnswer, type AskUiRequest, validateAskAnswers } from "../src/index.ts";

const request: AskUiRequest = {
  id: "ui_9",
  questions: [
    {
      id: "db",
      question: "Which database?",
      options: [{ label: "Postgres" }, { label: "SQLite" }],
      multi: false,
    },
    {
      id: "features",
      question: "Which features?",
      options: [{ label: "Auth" }, { label: "Billing" }, { label: "Search" }],
      multi: true,
    },
  ],
};

function answer(id: string, selected: string[], custom?: string): AskAnswer {
  return custom === undefined
    ? { id, selectedOptions: selected }
    : { id, selectedOptions: selected, customInput: custom };
}

describe("validateAskAnswers", () => {
  // spec: DMN-054
  test("accepts the documented answer shapes", () => {
    for (const answers of [
      [answer("db", [], "DuckDB"), answer("features", ["Auth", "Search"])],
      [answer("db", ["SQLite"]), answer("features", [])],
      [answer("db", []), answer("features", ["Billing"], "and more")],
      [answer("db", ["Postgres"], "   "), answer("features", ["Auth", "Billing", "Search"])],
    ]) {
      expect(validateAskAnswers(request, answers)).toBeUndefined();
    }
  });

  // spec: DMN-054
  test("requires one answer per question", () => {
    expect(validateAskAnswers(request, [answer("db", ["SQLite"])])).toBeString();
    const three = [answer("db", []), answer("features", []), answer("features", [])];
    expect(validateAskAnswers(request, three)).toBeString();
    expect(validateAskAnswers(request, [])).toBeString();
  });

  // spec: DMN-054
  test("requires question ids in request order", () => {
    expect(validateAskAnswers(request, [answer("features", []), answer("db", [])])).toBeString();
    expect(validateAskAnswers(request, [answer("db", []), answer("feature", [])])).toBeString();
  });

  // spec: DMN-054
  test("rejects unknown or inexact labels", () => {
    for (const label of ["MySQL", "sqlite", "SQLite ", ""]) {
      expect(validateAskAnswers(request, [answer("db", [label]), answer("features", [])])).toBeString();
    }
  });

  // spec: DMN-054
  test("rejects duplicate labels", () => {
    expect(validateAskAnswers(request, [answer("db", []), answer("features", ["Auth", "Auth"])])).toBeString();
  });

  // spec: DMN-054
  test("a single-select question takes at most one answer", () => {
    const twoOptions = [answer("db", ["Postgres", "SQLite"]), answer("features", [])];
    expect(validateAskAnswers(request, twoOptions)).toBeString();
    const optionAndCustom = [answer("db", ["Postgres"], "DuckDB"), answer("features", [])];
    expect(validateAskAnswers(request, optionAndCustom)).toBeString();
  });

  // spec: DMN-054
  test("errors name the offending question", () => {
    const error = validateAskAnswers(request, [answer("db", []), answer("features", ["Nope"])]);
    expect(error).toBe('answer "features" selected unknown option "Nope"');
  });
});
