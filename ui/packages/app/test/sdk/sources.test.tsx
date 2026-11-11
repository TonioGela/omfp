/** L4: the widget hooks of the SDK (`useWidget`, `useViews`, `useStored`, `useJob`, `useToolRun`) over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import type { JsonValue, ThreadEvent } from "@omfp/core";
import { type JobInfo, type JobState, jobTopic, viewsTopic } from "@omfp/protocol";
import { act, cleanup, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import {
  SELECTED_THREAD,
  type ToolRunTarget,
  useJob,
  useStored,
  useToolRun,
  useViews,
  useWidget,
  type WidgetInfoScope,
  WidgetScope,
} from "../../src/sdk/index.ts";
import { type CallRecord, renderWithHost, type TestHost, testHost } from "../support/host.tsx";

afterEach(cleanup);

const WS = "01W";
const SCOPE: WidgetInfoScope = { instance: "tests", reference: "./widgets/tests", workspaceId: WS };

function inScope(ui: ReactNode, scope: WidgetInfoScope = SCOPE): ReactNode {
  return <WidgetScope value={scope}>{ui}</WidgetScope>;
}

function text(testId: string): string | null {
  return screen.getByTestId(testId).textContent;
}

function only(calls: CallRecord[]): CallRecord {
  expect(calls.length).toBe(1);
  const [call] = calls;
  if (call === undefined) {
    throw new Error("no call");
  }
  return call;
}

function Scope() {
  const { instance, reference, workspaceId } = useWidget();
  return <span data-testid="scope">{[instance, reference, workspaceId].join(" ")}</span>;
}

function Counter() {
  const [count, setCount, loaded] = useStored("count", 0);
  return (
    <>
      <span data-testid="count">{`${count} ${loaded}`}</span>
      <button type="button" data-testid="inc" onClick={() => setCount((value) => (value ?? 0) + 1)} />
      <button type="button" data-testid="set" onClick={() => setCount(10)} />
      <button type="button" data-testid="clear" onClick={() => setCount(null)} />
    </>
  );
}

function ViewsProbe() {
  const views = useViews(WS);
  return <span data-testid="views">{views === undefined ? "none" : views.widgets.length}</span>;
}

describe("widget scope", () => {
  // spec: AGT-016
  test("useWidget gives the instance, reference and workspace of the slot", () => {
    renderWithHost(inScope(<Scope />));
    expect(text("scope")).toBe("tests ./widgets/tests 01W");
  });

  // spec: AGT-016
  test("useWidget outside a widget slot throws", () => {
    expect(() => renderWithHost(<Scope />)).toThrow("useWidget outside a widget slot");
  });

  // spec: PRT-040, AGT-016
  test("useViews follows views/<id> while mounted", () => {
    const { client, flush, view } = renderWithHost(<ViewsProbe />);
    expect(client.subscribed).toEqual([viewsTopic(WS)]);
    expect(text("views")).toBe("none");
    client.views(WS, { widgets: [{ source: "user", name: "clock", version: "1", entry: "index.tsx" }] });
    flush();
    expect(text("views")).toBe("1");
    view.unmount();
    expect(client.unsubscribed).toEqual([viewsTopic(WS)]);
  });
});

describe("useStored", () => {
  // spec: AGT-018
  test("reads the instance's value, initial until stored, and follows pushes", () => {
    const { client, flush } = renderWithHost(inScope(<Counter />));
    expect(text("count")).toBe("0 false");
    client.views(WS, { stored: { other: { count: 7 } } });
    flush();
    expect(text("count")).toBe("0 true");
    client.viewsEvents(WS, { kind: "widget_stored", instance: "tests", key: "count", value: 3 });
    flush();
    expect(text("count")).toBe("3 true");
    client.viewsEvents(WS, { kind: "widget_stored", instance: "tests", key: "count", value: null });
    flush();
    expect(text("count")).toBe("0 true");
  });

  // spec: AGT-018
  test("set stores a value, an updater's result or null with widgets.state_set", () => {
    const { client, flush } = renderWithHost(inScope(<Counter />));
    client.views(WS, { stored: { tests: { count: 4 } } });
    flush();

    act(() => screen.getByTestId("inc").click());
    act(() => screen.getByTestId("inc").click());
    act(() => screen.getByTestId("set").click());
    act(() => screen.getByTestId("clear").click());

    expect(client.callsTo("widgets.state_set").map((call) => call.params)).toEqual([
      { workspaceId: WS, instance: "tests", key: "count", value: 5 },
      // The second updater starts from the value set last, before its push arrived.
      { workspaceId: WS, instance: "tests", key: "count", value: 6 },
      { workspaceId: WS, instance: "tests", key: "count", value: 10 },
      { workspaceId: WS, instance: "tests", key: "count", value: null },
    ]);
  });
});

function info(id: string, status: JobInfo["status"] = { state: "running" }): JobInfo {
  return { id, workspaceId: WS, command: "make test", startedAt: 1, status };
}

function jobState(id: string, lines: string[], status?: JobInfo["status"]): JobState {
  return { job: info(id, status), lines: lines.map((line) => ({ stream: "stdout", text: line })), dropped: 0 };
}

function JobProbe() {
  const job = useJob("make test");
  return (
    <>
      <span data-testid="job">
        {[job.running, job.exitCode, job.signal, job.lines.join("|")].map((part) => String(part)).join(" ")}
      </span>
      <button type="button" data-testid="restart" onClick={job.restart} />
      <button type="button" data-testid="kill" onClick={job.kill} />
    </>
  );
}

/** Renders the job probe with `stored` as the views state's values of the instance. */
function renderJob(stored: Record<string, unknown> | undefined): TestHost {
  const test = renderWithHost(inScope(<JobProbe />), testHost());
  if (stored !== undefined) {
    test.client.views(WS, { stored: { tests: stored } });
    test.flush();
  }
  return test;
}

describe("useJob", () => {
  // spec: AGT-019
  test("without a stored job it starts one once the views state is known and stores its id", async () => {
    const { client, flush } = renderJob(undefined);
    expect(client.callsTo("jobs.start")).toEqual([]);
    client.views(WS);
    flush();

    const start = only(client.callsTo("jobs.start"));
    expect(start.params).toEqual({ workspaceId: WS, command: "make test" });
    flush();
    expect(client.callsTo("jobs.start").length).toBe(1);
    await act(async () => start.resolve(info("01JOB")));
    expect(only(client.callsTo("widgets.state_set")).params).toEqual({
      workspaceId: WS,
      instance: "tests",
      key: "job:make test",
      value: "01JOB",
    });

    client.viewsEvents(WS, { kind: "widget_stored", instance: "tests", key: "job:make test", value: "01JOB" });
    flush();
    expect(client.subscribed).toContain(jobTopic("01JOB"));
    client.job(jobState("01JOB", ["one"]));
    client.jobEvents("01JOB", { kind: "job_output", stream: "stderr", text: "two" });
    flush();
    expect(text("job")).toBe("true null null one|two");
    client.jobEvents("01JOB", { kind: "job_exited", code: 2, signal: null, endedAt: 5 });
    flush();
    expect(text("job")).toBe("false 2 null one|two");
    expect(client.callsTo("jobs.start").length).toBe(1);
  });

  // spec: AGT-019
  test("a stored job is followed, running or finished, without starting another", () => {
    const { client, flush } = renderJob({ "job:make test": "01OLD" });
    client.job(jobState("01OLD", ["done"], { state: "exited", code: null, signal: 15, endedAt: 3 }));
    flush();
    expect(text("job")).toBe("false null 15 done");
    expect(client.callsTo("jobs.start")).toEqual([]);
  });

  // spec: AGT-019
  test("a stored job the daemon no longer knows is replaced by a new one", () => {
    const { client, flush } = renderJob({ "job:make test": "01GONE" });
    expect(client.callsTo("jobs.start")).toEqual([]);
    client.failTopic(jobTopic("01GONE"), { code: "not_found", message: "no job 01GONE" });
    flush();
    expect(only(client.callsTo("jobs.start")).params).toEqual({ workspaceId: WS, command: "make test" });
  });

  // spec: AGT-019
  test("restart kills the running job, then starts and stores a new one; kill stops it", async () => {
    const { client, flush } = renderJob({ "job:make test": "01JOB" });
    client.job(jobState("01JOB", []));
    flush();

    act(() => screen.getByTestId("kill").click());
    expect(only(client.callsTo("jobs.kill")).params).toEqual({ jobId: "01JOB" });

    act(() => screen.getByTestId("restart").click());
    const kills = client.callsTo("jobs.kill");
    expect(kills.length).toBe(2);
    expect(client.callsTo("jobs.start")).toEqual([]);
    await act(async () => kills[1]?.resolve({}));
    const start = only(client.callsTo("jobs.start"));
    await act(async () => start.resolve(info("01NEW")));
    expect(only(client.callsTo("widgets.state_set")).params).toMatchObject({ key: "job:make test", value: "01NEW" });
  });

  // spec: AGT-019
  test("restart of a finished job starts a new one without killing", () => {
    const { client, flush } = renderJob({ "job:make test": "01JOB" });
    client.job(jobState("01JOB", [], { state: "exited", code: 0, signal: null, endedAt: 2 }));
    flush();
    act(() => screen.getByTestId("restart").click());
    expect(client.callsTo("jobs.kill")).toEqual([]);
    expect(client.callsTo("jobs.start").length).toBe(1);
  });
});

const THREAD = "01T";

function omp(frame: { [key: string]: JsonValue }): ThreadEvent {
  return { kind: "omp", frame };
}

const START = omp({
  type: "tool_execution_start",
  toolCallId: "c1",
  toolName: "bash",
  args: { command: "cargo nextest run" },
});
const UPDATE = omp({
  type: "tool_execution_update",
  toolCallId: "c1",
  toolName: "bash",
  args: { command: "cargo nextest run" },
  partialResult: { content: [{ type: "text", text: "Starting 3 tests\nPASS a\n" }] },
});
const END = omp({
  type: "tool_execution_end",
  toolCallId: "c1",
  toolName: "bash",
  result: { content: [{ type: "text", text: "Starting 3 tests\nPASS a\nFAIL b" }] },
  isError: true,
});
const OTHER = omp({ type: "tool_execution_start", toolCallId: "c2", toolName: "read", args: { path: "nextest.toml" } });

function ToolProbe({ target }: { target: ToolRunTarget }) {
  const run = useToolRun(target);
  return (
    <span data-testid="run">
      {run === undefined
        ? "none"
        : [run.toolCallId, run.toolName, run.status, run.isError, run.lines.join("|")].map(String).join(" ")}
    </span>
  );
}

describe("useToolRun", () => {
  // spec: AGT-020
  test("follows a run of the selected thread by id: partial output while running, the result once ended", () => {
    const { client, flush, host } = renderWithHost(<ToolProbe target="c1" />);
    act(() => host.window.set(SELECTED_THREAD, THREAD));
    client.thread(THREAD, [START, UPDATE]);
    flush();
    expect(text("run")).toBe("c1 bash running false Starting 3 tests|PASS a");
    client.threadEvents(THREAD, END);
    flush();
    expect(text("run")).toBe("c1 bash failed true Starting 3 tests|PASS a|FAIL b");
  });

  // spec: AGT-020
  test("matches the latest run whose command matches, in a given thread", () => {
    const { client, flush } = renderWithHost(<ToolProbe target={{ match: /nextest/, threadId: THREAD }} />);
    client.thread(THREAD, [START, OTHER]);
    flush();
    // The `read` run matches through its JSON arguments and is the latest.
    expect(text("run")).toBe("c2 read running false ");
    cleanup();

    const second = renderWithHost(<ToolProbe target={{ match: "cargo", threadId: THREAD }} />);
    second.client.thread(THREAD, [START, UPDATE, OTHER]);
    second.flush();
    expect(text("run")).toBe("c1 bash running false Starting 3 tests|PASS a");
  });

  // spec: AGT-020
  test("is undefined without a matching run", () => {
    const { client, flush } = renderWithHost(<ToolProbe target={{ match: /nope/, threadId: THREAD }} />);
    client.thread(THREAD, [START]);
    flush();
    expect(text("run")).toBe("none");
  });
});
