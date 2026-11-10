/** TST-032: the HTML snapshot normalizer, and normalized snapshots of the transcript over recorded fixtures. */

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { cleanup, screen } from "@testing-library/react";
import { TranscriptWidget } from "../../../src/widgets/chat/index.ts";
import { snapshotHtml } from "../../support/snapshot.ts";
import { fixtureEvents } from "./fixtures.ts";
import { renderThread } from "./harness.tsx";
import { fakeLayout } from "./layout.ts";

function element(html: string): Element {
  const template = document.createElement("template");
  template.innerHTML = html;
  const first = template.content.firstElementChild;
  if (first === null) {
    throw new Error("no element");
  }
  return first;
}

describe("snapshot normalizer", () => {
  // spec: TST-032
  test("generated ids and their references are numbered by first appearance", () => {
    const one = element(
      '<div><label id="«r3»" for="react-aria-:r9:">L</label><input id="react-aria-:r9:" aria-labelledby="«r3» extra"></div>',
    );
    const two = element(
      '<div><label id="«r7»" for="react-aria-:r12:">L</label><input id="react-aria-:r12:" aria-labelledby="«r7» extra"></div>',
    );
    expect(snapshotHtml(one)).toBe(snapshotHtml(two));
    expect(snapshotHtml(one)).toBe(
      '<div><label id="id-1" for="id-2">L</label><input id="id-2" aria-labelledby="id-1 id-3"></div>',
    );
    expect(one.querySelector("label")?.id).toBe("«r3»");
  });

  // spec: TST-032
  test("timestamps are stripped", () => {
    const html = snapshotHtml(
      element('<p><time datetime="1731230810707">at 2026-11-10T09:26:50.707Z</time><span data-time="5">x</span></p>'),
    );
    expect(html).toBe(
      '<p><time datetime="<timestamp>">at <timestamp></time><span data-time="<timestamp>">x</span></p>',
    );
  });
});

describe("transcript snapshots", () => {
  let restore: () => void = () => {};
  beforeEach(() => {
    restore = fakeLayout();
  });
  afterEach(() => {
    cleanup();
    restore();
  });

  // spec: TST-032, UI-050
  test("a bash tool call", () => {
    renderThread(<TranscriptWidget />, fixtureEvents("bash_tool_call"));
    expect(snapshotHtml(screen.getByTestId("transcript"))).toMatchSnapshot();
  });

  // spec: TST-032, UI-040
  test("a text reply", () => {
    renderThread(<TranscriptWidget />, fixtureEvents("text_reply"));
    expect(snapshotHtml(screen.getByTestId("transcript"))).toMatchSnapshot();
  });
});
