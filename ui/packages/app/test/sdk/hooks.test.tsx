/** L4: the SDK hooks over a fake client. */

import { afterEach, describe, expect, test } from "bun:test";
import type { ThreadSummary } from "@omfp/protocol";
import { threadTopic } from "@omfp/protocol";
import { act, cleanup, screen } from "@testing-library/react";
import { useState } from "react";
import {
  CommandRegistry,
  SCOPE_ATTRIBUTE,
  useCommand,
  useRunCommand,
  useThread,
  useThreads,
  useWindowState,
  windowKey,
} from "../../src/sdk/index.ts";
import { renderWithHost, testHost } from "../support/host.tsx";

afterEach(cleanup);

function ThreadProbe({ id }: { id: string }) {
  const thread = useThread(id);
  return <span data-testid={`probe-${id}`}>{thread === undefined ? "none" : thread.state.transcript.length}</span>;
}

function TwoProbes() {
  const [both, setBoth] = useState(true);
  return (
    <>
      <ThreadProbe id="01T" />
      {both && <ThreadProbe id="01T" />}
      <button type="button" data-testid="drop" onClick={() => setBoth(false)} />
    </>
  );
}

function ThreadCount() {
  return <span data-testid="count">{useThreads().size}</span>;
}

const COUNTER = windowKey("test/counter", 0);

function Counter({ testid }: { testid: string }) {
  const [count, setCount] = useWindowState(COUNTER);
  return (
    <button type="button" data-testid={testid} onClick={() => setCount((previous) => previous + 1)}>
      {count}
    </button>
  );
}

const ran: string[] = [];

function Scoped() {
  useCommand({
    id: "nav.next",
    title: "Next message",
    scope: "transcript",
    run: () => {
      ran.push("transcript");
    },
  });
  useCommand({
    id: "nav.next",
    title: "Next thread",
    run: () => {
      ran.push("global");
    },
  });
  const run = useRunCommand();
  return (
    <>
      <div {...{ [SCOPE_ATTRIBUTE]: "transcript" }}>
        <button type="button" data-testid="inside" />
      </div>
      <button type="button" data-testid="outside" />
      <button type="button" data-testid="run" onClick={() => run("nav.next")} />
    </>
  );
}

function summary(id: string): ThreadSummary {
  return {
    id,
    title: id,
    titleSource: "set",
    workspaceId: "01W",
    process: { state: "stopped" },
    settled: true,
    needsAttention: false,
    provenance: { createdBy: { kind: "user" }, client: "test" },
    createdAt: 1,
    updatedAt: 1,
  };
}

describe("hooks", () => {
  // spec: UI-010
  test("useThread follows thread/<id> while any widget shows it and renders the reducer state", () => {
    const topic = threadTopic("01T");
    const { client, flush, view } = renderWithHost(<TwoProbes />, testHost());
    expect(client.subscribed).toEqual([topic]);

    client.thread("01T", []);
    flush();
    expect(screen.getAllByTestId("probe-01T").map((node) => node.textContent)).toEqual(["0", "0"]);

    act(() => screen.getByTestId("drop").click());
    expect(client.unsubscribed).toEqual([]);
    view.unmount();
    expect(client.unsubscribed).toEqual([topic]);
  });

  // spec: UI-012
  test("useThreads renders the index of the last committed frame only", () => {
    const { client, flush } = renderWithHost(<ThreadCount />);
    client.index({ threads: [summary("01A")] });
    expect(screen.getByTestId("count").textContent).toBe("0");
    flush();
    expect(screen.getByTestId("count").textContent).toBe("1");
  });

  test("useWindowState shares a slot between widgets", () => {
    renderWithHost(
      <>
        <Counter testid="a" />
        <Counter testid="b" />
      </>,
    );
    act(() => screen.getByTestId("a").click());
    expect(screen.getByTestId("b").textContent).toBe("1");
  });

  // spec: UI-095
  test("a command resolves in the scope around the focused element first, then globally", () => {
    ran.length = 0;
    renderWithHost(<Scoped />);
    const press = () => {
      // Keeps the focus where it is, as a key binding would.
      const run = screen.getByTestId("run");
      act(() => {
        run.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      });
    };
    act(() => screen.getByTestId("inside").focus());
    press();
    act(() => screen.getByTestId("outside").focus());
    press();
    expect(ran).toEqual(["transcript", "global"]);
  });
});

describe("command registry", () => {
  test("aliases, disabled commands and unregistering", () => {
    const registry = new CommandRegistry();
    const args: string[][] = [];
    const off = registry.register({
      id: "theme.set",
      title: "Theme",
      aliases: ["theme"],
      run: (given) => {
        args.push([...given]);
      },
    });
    registry.register({ id: "thread.archive", title: "Archive", enabled: () => false, run: () => {} });

    expect(registry.run("theme", ["dark"])).toBe(true);
    expect(registry.run("thread.archive")).toBe(false);
    off();
    expect(registry.run("theme", ["light"])).toBe(false);
    expect(args).toEqual([["dark"]]);
  });
});
