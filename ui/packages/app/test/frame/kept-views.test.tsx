/** L4: the views the frame keeps mounted, hidden, after switching away from them (UI-390, UI-391, UI-392). */

import { afterEach, describe, expect, test } from "bun:test";
import type { CallMethod } from "@omfp/client";
import type { FileContent, ThreadSummary } from "@omfp/protocol";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";
import { useEffect, useState } from "react";
import { Frame } from "../../src/frame/frame.tsx";
import {
  chooseView,
  type FilePoll,
  type OmfpHost,
  SELECTED_THREAD,
  useCommand,
  useFile,
  type WidgetComponent,
} from "../../src/sdk/index.ts";
import { KEPT_VIEWS } from "../../src/view/kept.ts";
import type { WidgetRegistry } from "../../src/view/resolve.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { ManualTimers, renderWithHost, testHost } from "../support/host.tsx";

afterEach(cleanup);

/** The host's file poll, run by the test. */
class ManualPoll {
  readonly checks = new Set<() => void>();
  readonly poll: FilePoll = (check) => {
    this.checks.add(check);
    return () => {
      this.checks.delete(check);
    };
  };

  run(): void {
    act(() => {
      for (const check of [...this.checks]) {
        check();
      }
    });
  }
}

/** Counts the counter widgets created (each mount takes the next number) and their effects running now. */
const counters = { created: 0, effects: 0 };

/** A widget with state, a mount number, a field to focus, and an effect it counts while it runs. */
const Counter: WidgetComponent = () => {
  const [mount] = useState(() => {
    counters.created += 1;
    return counters.created;
  });
  const [count, setCount] = useState(0);
  useEffect(() => {
    counters.effects += 1;
    return () => {
      counters.effects -= 1;
    };
  }, []);
  return (
    <div data-testid="counter" data-mount={mount}>
      <button type="button" data-testid="counter-add" onClick={() => setCount((value) => value + 1)}>
        {count}
      </button>
      <input data-testid="counter-input" />
    </div>
  );
};

/** A widget polling a file (`useFile` follows it through the host's poll). */
const Poller: WidgetComponent = () => {
  const { file } = useFile("notes.md");
  return <p data-testid="poller">{file?.content ?? ""}</p>;
};

/** A widget registering a command. */
const Pinger: WidgetComponent = () => {
  useCommand({ id: "test.ping", title: "Ping", run: () => {} });
  return <p data-testid="pinger">ping</p>;
};

const stub =
  (name: string): WidgetComponent =>
  () => <p data-testid={`stub-${name}`}>{name}</p>;

const registry: WidgetRegistry = new Map<string, WidgetComponent>([
  ["omfp:test-counter", Counter],
  ["omfp:test-poller", Poller],
  ["omfp:test-pinger", Pinger],
  ["omfp:header", stub("header")],
  ["omfp:chat", stub("chat")],
]);

const MANIFEST = {
  default: "a",
  views: {
    a: { title: "A", layout: ["row", {}, "omfp:test-counter", "omfp:test-poller"] },
    b: { title: "B", layout: "omfp:test-pinger" },
    c: { title: "C", layout: "omfp:header" },
    d: { title: "D", layout: "omfp:chat" },
    e: { title: "E", layout: ["col", {}, "./widgets/clock", "omfp:header"] },
  },
};

const NOTES: FileContent = {
  path: "notes.md",
  kind: "text",
  size: 5,
  hash: "h1",
  unchanged: false,
  content: "notes",
  truncated: false,
  repo: null,
  head: null,
};

const threads: ThreadSummary[] = [thread("t1", "one", "w1"), thread("t2", "two", "w2")];

/** A clock module widget (`./widgets/clock`), reported to the daemon while shown (AGT-026). */
const Clock: WidgetComponent = () => <p data-testid="clock">12:00</p>;

function setup() {
  counters.created = 0;
  counters.effects = 0;
  const poll = new ManualPoll();
  const reportTimers = new ManualTimers();
  const base = testHost(undefined, {
    importer: () => Promise.resolve({ default: Clock }),
    reportTimers,
    filePoll: poll.poll,
  });
  for (const method of ["widgets.report"] as const satisfies readonly CallMethod[]) {
    base.client.answers.set(method, () => ({}));
  }
  base.client.answers.set("workspaces.read_file", () => NOTES);
  const test = renderWithHost(<Frame registry={registry} now={() => 0} />, base);
  test.client.connected();
  test.client.index({ workspaces: [workspace("w1"), workspace("w2")], threads });
  act(() => test.host.window.set(SELECTED_THREAD, "t1"));
  const widgets = [{ name: "clock", source: "workspace" as const, version: "v1", entry: "index.tsx" }];
  test.client.views("w1", { manifest: MANIFEST, widgets });
  test.client.views("w2", { manifest: MANIFEST, widgets });
  test.flush();
  return {
    ...test,
    poll,
    /** The widgets the latest `widgets.report` lists, after the report timers ran. */
    reported: () => {
      reportTimers.run();
      const last = test.client.callsTo("widgets.report").at(-1)?.params as
        | { widgets: { reference: string }[] }
        | undefined;
      return last?.widgets.map((widget) => widget.reference);
    },
  };
}

function show(host: OmfpHost, id: string): void {
  act(() => chooseView(host, id));
}

/** The shown view's id, then the hidden ones' in DOM order. */
function mounted(): { shown: string | undefined; kept: (string | undefined)[] } {
  return {
    shown: screen.getByTestId("view").dataset.view,
    kept: screen.queryAllByTestId("kept-view").map((view) => view.dataset.view),
  };
}

/** Lets module imports and the renders they cause settle. */
async function settle(test: { flush: () => void }): Promise<void> {
  await act(async () => {
    const { promise, resolve } = Promise.withResolvers<void>();
    setTimeout(resolve, 0);
    await promise;
  });
  test.flush();
}

describe("kept views", () => {
  // spec: UI-390
  test("switching A → B → A shows A as it was: the same widgets, state and DOM, no new mount", () => {
    const { host } = setup();
    expect(mounted()).toEqual({ shown: "a", kept: [] });
    fireEvent.click(screen.getByTestId("counter-add"));
    fireEvent.click(screen.getByTestId("counter-add"));
    const counter = screen.getByTestId("counter");
    expect(counter.dataset.mount).toBe("1");
    show(host, "b");
    expect(mounted()).toEqual({ shown: "b", kept: ["a"] });
    // Kept in the DOM, not shown.
    expect(within(screen.getByTestId("kept-view")).getByTestId("counter").dataset.mount).toBe("1");
    show(host, "a");
    expect(mounted()).toEqual({ shown: "a", kept: ["b"] });
    expect(screen.getByTestId("counter").dataset.mount).toBe("1");
    expect(screen.getByTestId("counter") === counter).toBe(true);
    expect(screen.getByTestId("counter-add").textContent).toBe("2");
    expect(counters.created).toBe(1);
  });

  // spec: UI-390
  test("a hidden view's effects are cleaned up: its polls stop and run again once it shows", async () => {
    const test = setup();
    const { host, client, poll } = test;
    await settle(test);
    expect(screen.getByTestId("poller").textContent).toBe("notes");
    expect(counters.effects).toBe(1);
    expect(poll.checks.size).toBe(1);
    const reads = () => client.callsTo("workspaces.read_file").length;
    poll.run();
    expect(reads()).toBe(2);
    show(host, "b");
    expect(counters.effects).toBe(0);
    expect(poll.checks.size).toBe(0);
    poll.run();
    expect(reads()).toBe(2);
    show(host, "a");
    expect(counters.effects).toBe(1);
    expect(poll.checks.size).toBe(1);
    // Shown again, it reads the file again (its effect ran again) while showing what it had.
    expect(reads()).toBe(3);
    expect(screen.getByTestId("poller").textContent).toBe("notes");
  });

  // spec: UI-390, UI-387
  test(`at most ${KEPT_VIEWS} views stay kept besides the shown one: the least recently shown is dropped`, async () => {
    const test = setup();
    const { host } = test;
    await settle(test);
    show(host, "b");
    show(host, "c");
    expect(mounted()).toEqual({ shown: "c", kept: ["a", "b"] });
    show(host, "d");
    // `a` was shown least recently: unmounted.
    expect(mounted()).toEqual({ shown: "d", kept: ["b", "c"] });
    expect(counters.effects).toBe(0);
    show(host, "b");
    show(host, "a");
    // A new mount of the counter; `b` stays (shown just before), `c` goes.
    expect(mounted()).toEqual({ shown: "a", kept: ["b", "d"] });
    expect(screen.getByTestId("counter").dataset.mount).toBe("2");
    // The new mount of the poller shows the window's last copy of its file while it reads it again.
    expect(screen.getByTestId("poller").textContent).toBe("notes");
  });

  // spec: UI-390
  test("views of another workspace and views no longer listed are dropped", () => {
    const test = setup();
    const { host, client, flush } = test;
    show(host, "b");
    show(host, "c");
    expect(mounted()).toEqual({ shown: "c", kept: ["a", "b"] });
    client.viewsEvents("w1", {
      kind: "views_manifest",
      manifest: { ...MANIFEST, views: { a: MANIFEST.views.a, c: MANIFEST.views.c } },
      error: null,
    });
    flush();
    expect(mounted()).toEqual({ shown: "c", kept: ["a"] });
    act(() => host.window.set(SELECTED_THREAD, "t2"));
    flush();
    expect(mounted()).toEqual({ shown: "a", kept: [] });
    expect(counters.created).toBe(2);
  });

  // spec: UI-391
  test("the commands of a hidden view's widgets are not registered; they are again once it shows", () => {
    const { host } = setup();
    expect(host.commands.resolve("test.ping")).toBeUndefined();
    show(host, "b");
    expect(host.commands.resolve("test.ping")?.title).toBe("Ping");
    show(host, "a");
    expect(screen.queryAllByTestId("pinger").length).toBe(1);
    expect(host.commands.resolve("test.ping")).toBeUndefined();
    show(host, "b");
    expect(host.commands.resolve("test.ping")?.title).toBe("Ping");
  });

  // spec: UI-391
  test("a hidden view gives up the focus and gets it back where it was when it shows again", () => {
    const { host } = setup();
    const input = screen.getByTestId("counter-input");
    act(() => input.focus());
    expect(document.activeElement?.getAttribute("data-testid")).toBe("counter-input");
    show(host, "b");
    expect(document.activeElement === document.body).toBe(true);
    show(host, "a");
    expect(document.activeElement?.getAttribute("data-testid")).toBe("counter-input");
  });

  // spec: UI-391, AGT-026
  test("a hidden view's module widgets are not reported as shown; they are again once it shows", async () => {
    const test = setup();
    show(test.host, "e");
    await settle(test);
    expect(screen.getByTestId("clock").textContent).toBe("12:00");
    expect(test.reported()).toEqual(["./widgets/clock"]);
    show(test.host, "a");
    expect(screen.queryAllByTestId("clock").length).toBe(1);
    expect(test.reported()).toEqual([]);
    show(test.host, "e");
    expect(test.reported()).toEqual(["./widgets/clock"]);
  });
});
