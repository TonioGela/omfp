/** L4: the window's last results of the SDK's data hooks, shown again while they refresh (UI-392). */

import { afterEach, describe, expect, test } from "bun:test";
import { CallError } from "@omfp/client";
import { type FileContent, type JobLine, type JobState, viewsTopic } from "@omfp/protocol";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import {
  OmfpProvider,
  ResultCache,
  SELECTED_THREAD,
  useCall,
  useFile,
  useJob,
  type WidgetInfoScope,
  WidgetScope,
} from "../../src/sdk/index.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { type CallRecord, type TestHost, testHost } from "../support/host.tsx";

afterEach(cleanup);

const WS = "w1";
const SCOPE: WidgetInfoScope = { instance: "notes", reference: "./widgets/notes", workspaceId: WS };

/** Mounts `ui` in the widget slot `scope` of `test`'s host; returns the function unmounting it. */
function mount(test: TestHost, ui: ReactNode, scope: WidgetInfoScope = SCOPE): () => void {
  const { unmount } = render(
    <OmfpProvider host={test.host}>
      <WidgetScope value={scope}>{ui}</WidgetScope>
    </OmfpProvider>,
  );
  return unmount;
}

function text(testId: string): string | null {
  return screen.getByTestId(testId).textContent;
}

function last(calls: CallRecord[]): CallRecord {
  const call = calls.at(-1);
  if (call === undefined) {
    throw new Error("no call");
  }
  return call;
}

describe("ResultCache", () => {
  // spec: UI-392
  test("keeps the most recently used results within its entry and size bounds; a result over the size alone is not kept", () => {
    const cache = new ResultCache(2, 100);
    cache.set("a", "x".repeat(10));
    cache.set("b", "y".repeat(10));
    // Using `a` makes `b` the least recently used.
    expect(cache.get("a")?.value).toBe("x".repeat(10));
    cache.set("c", 1);
    expect([cache.get("a") !== undefined, cache.get("b") !== undefined, cache.get("c") !== undefined]).toEqual([
      true,
      false,
      true,
    ]);
    // Over the size: the oldest go until it fits.
    cache.set("d", "z".repeat(80));
    expect(cache.stats).toEqual({ entries: 2, size: 1 + 82 });
    expect(cache.get("a")).toBeUndefined();
    // Too big to keep: not kept, and the key's previous value is gone.
    cache.set("c", "w".repeat(200));
    expect(cache.get("c")).toBeUndefined();
    expect(cache.stats).toEqual({ entries: 1, size: 82 });
  });
});

function Session({ threadId }: { threadId: string | null }) {
  const session = useCall("thread.session", threadId === null ? null : { threadId });
  const error = session.error instanceof CallError ? session.error.message : String(session.error ?? "");
  return (
    <span data-testid="session">
      {[session.data?.sessionId ?? "-", session.refreshing ? "refreshing" : "done", error].join(" ")}
    </span>
  );
}

function sessionOf(sessionId: string): unknown {
  return { sessionId };
}

describe("useCall(method, params)", () => {
  // spec: UI-392
  test("a new mount shows the widget's last result at once, refreshing, then the new one; a failure keeps it", async () => {
    const test = testHost();
    const calls = () => test.client.callsTo("thread.session");
    let unmount = mount(test, <Session threadId="t1" />);
    expect(text("session")).toBe("- refreshing ");
    expect(calls().map((call) => call.params)).toEqual([{ threadId: "t1" }]);
    await act(async () => last(calls()).resolve(sessionOf("s1")));
    expect(text("session")).toBe("s1 done ");
    unmount();

    // Mounted again: the kept result at once, while the call runs.
    unmount = mount(test, <Session threadId="t1" />);
    expect(text("session")).toBe("s1 refreshing ");
    expect(calls().length).toBe(2);
    await act(async () => last(calls()).resolve(sessionOf("s2")));
    expect(text("session")).toBe("s2 done ");
    unmount();

    // A failure keeps the data, with the error.
    unmount = mount(test, <Session threadId="t1" />);
    expect(text("session")).toBe("s2 refreshing ");
    const failure = new CallError({ kind: "closed" });
    await act(async () => last(calls()).reject(failure));
    expect(text("session")).toBe(`s2 done ${failure.message}`);
    unmount();
  });

  // spec: UI-392
  test("results are kept per widget instance and params; null params call nothing", async () => {
    const test = testHost();
    const calls = () => test.client.callsTo("thread.session");
    const unmount = mount(test, <Session threadId="t1" />);
    await act(async () => last(calls()).resolve(sessionOf("s1")));
    unmount();
    // Another instance of the widget has none.
    mount(test, <Session threadId="t1" />, { ...SCOPE, instance: "other" });
    expect(text("session")).toBe("- refreshing ");
    cleanup();
    // Other params have none either; without params there is no call.
    mount(test, <Session threadId="t2" />);
    expect(text("session")).toBe("- refreshing ");
    cleanup();
    const before = calls().length;
    mount(test, <Session threadId={null} />);
    expect(text("session")).toBe("- done ");
    expect(calls().length).toBe(before);
  });
});

const NOTES: FileContent = {
  path: "notes.md",
  kind: "text",
  size: 5,
  hash: "h1",
  unchanged: false,
  content: "first",
  truncated: false,
  repo: null,
  head: null,
};

function Notes() {
  const { status, file, refreshing } = useFile("notes.md", { follow: false });
  return (
    <span data-testid="notes">{[status, file?.content ?? "-", refreshing ? "refreshing" : "done"].join(" ")}</span>
  );
}

function inWorkspace(): TestHost {
  const test = testHost();
  test.client.connected();
  test.client.index({ workspaces: [workspace(WS)], threads: [thread("t1", "one", WS)] });
  test.host.window.set(SELECTED_THREAD, "t1");
  test.flush();
  return test;
}

describe("useFile", () => {
  // spec: UI-392
  test("a new mount shows the window's last copy of the file at once, refreshing, then what it reads", async () => {
    const test = inWorkspace();
    const reads = () => test.client.callsTo("workspaces.read_file");
    let unmount = mount(test, <Notes />);
    expect(text("notes")).toBe("loading - refreshing");
    await act(async () => last(reads()).resolve(NOTES));
    expect(text("notes")).toBe("ready first done");
    unmount();

    unmount = mount(test, <Notes />);
    expect(text("notes")).toBe("ready first refreshing");
    await act(async () => last(reads()).resolve({ ...NOTES, content: "second", hash: "h2" }));
    expect(text("notes")).toBe("ready second done");
    unmount();
    // A file gone is no longer kept.
    unmount = mount(test, <Notes />);
    expect(text("notes")).toBe("ready second refreshing");
    await act(async () =>
      last(reads()).reject(new CallError({ kind: "daemon", error: { code: "not_found", message: "gone" } })),
    );
    expect(text("notes")).toBe("missing - done");
    unmount();
    mount(test, <Notes />);
    expect(text("notes")).toBe("loading - refreshing");
  });
});

const out = (line: string): JobLine => ({ stream: "stdout", text: line });

function jobState(id: string, lines: JobLine[]): JobState {
  return {
    job: { id, workspaceId: WS, command: "make test", startedAt: 1, status: { state: "running" } },
    lines,
    dropped: 0,
  };
}

function JobLines() {
  const job = useJob("make test");
  return (
    <span data-testid="job">
      {[job.status, job.lines.join("|") || "-", job.refreshing ? "refreshing" : "done"].join(" ")}
    </span>
  );
}

describe("useJob", () => {
  // spec: UI-392
  test("a new mount shows the window's last copy of the job until its topic's snapshot arrives", () => {
    const test = testHost();
    // The frame follows the current workspace's views topic, whatever widgets mount.
    test.host.topics.retain(viewsTopic(WS));
    test.client.views(WS, { stored: { notes: { "job:make test": { id: "01JOB", runs: 1 } } } });
    test.flush();
    let unmount = mount(test, <JobLines />);
    expect(text("job")).toBe("pending - done");
    test.client.job(jobState("01JOB", [out("one"), out("two")]));
    test.flush();
    expect(text("job")).toBe("running one|two done");
    unmount();
    test.flush();
    // The topic was released: its state left the model.
    expect(test.client.unsubscribed).toContain("job/01JOB");

    unmount = mount(test, <JobLines />);
    expect(text("job")).toBe("running one|two refreshing");
    test.client.job(jobState("01JOB", [out("one"), out("two"), out("three")]));
    test.flush();
    expect(text("job")).toBe("running one|two|three done");
    unmount();
  });
});
