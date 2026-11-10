/** L0 tests of the client model: index and thread events, snapshots, replay, resync and change tracking. */

import { describe, expect, test } from "bun:test";
import { replay, type ThreadEvent } from "@omfp/core";
import { type EventFrame, INDEX_TOPIC, type SubscribeResult, threadTopic } from "@omfp/protocol";
import { type Dirty, isDirtyEmpty, Model, type ThreadView } from "../src/index.ts";
import { FIXTURES, fixture, indexFrame, indexState, summary, threadFrame, workspace } from "./support.ts";

const THREAD = "01THREAD";
const TOPIC = threadTopic(THREAD);

function snapshot(epoch: string, seq: number, state: unknown): SubscribeResult {
  return { mode: "snapshot", epoch, seq, state };
}

/** A model following `thread/<THREAD>` from a snapshot of `events` at `epoch`/`events.length`. */
function following(epoch: string, events: ThreadEvent[]): Model {
  const model = new Model();
  expect(model.track(TOPIC)).toBe(true);
  const started = model.applySubscribed(TOPIC, snapshot(epoch, events.length, replay(events)));
  expect(started).toEqual({ ok: true, mode: "snapshot" });
  return model;
}

function applyAll(model: Model, epoch: string, firstSeq: number, events: ThreadEvent[]): void {
  for (const [offset, event] of events.entries()) {
    const seq = firstSeq + offset;
    expect({ seq, applied: model.applyEvent(threadFrame(THREAD, epoch, seq, event)) }).toEqual({
      seq,
      applied: "applied",
    });
  }
}

function view(model: Model): ThreadView {
  const thread = model.thread(THREAD);
  if (thread === undefined) {
    throw new Error("thread not followed");
  }
  return thread;
}

function eventAt(events: ThreadEvent[], at: number): ThreadEvent {
  const event = events[at];
  if (event === undefined) {
    throw new Error(`no event ${at}`);
  }
  return event;
}

describe("model", () => {
  // spec: UI-010, PRT-021
  test("a snapshot plus the following events equals the reducer over every event", () => {
    for (const name of FIXTURES) {
      const events = fixture(name);
      for (const split of [0, Math.floor(events.length / 2), events.length]) {
        const model = following("e1", events.slice(0, split));

        applyAll(model, "e1", split + 1, events.slice(split));

        expect({ name, split, state: view(model).state }).toEqual({ name, split, state: replay(events) });
        expect(model.topic(TOPIC)?.position).toEqual({ epoch: "e1", seq: events.length });
      }
    }
  });

  // spec: PRT-020, UI-003
  test("after a connection loss the topic resubscribes from its position and replay continues", () => {
    const events = fixture("bash_tool_call");
    const model = following("e1", events.slice(0, 10));
    applyAll(model, "e1", 11, events.slice(10, 20));

    model.connectionLost();

    expect(model.topic(TOPIC)?.sync).toEqual({ kind: "subscribing" });
    // Events of the old connection no longer apply.
    expect(model.applyEvent(threadFrame(THREAD, "e1", 21, eventAt(events, 20)))).toBe("ignored");
    expect(model.subscribeParams(TOPIC)?.since).toEqual({ epoch: "e1", seq: 20 });
    const replayed = model.applySubscribed(TOPIC, { mode: "replay", epoch: "e1", fromSeq: 21 });
    expect(replayed).toEqual({ ok: true, mode: "replay" });
    applyAll(model, "e1", 21, events.slice(20));
    expect(view(model).state).toEqual(replay(events));
    expect(model.topic(TOPIC)?.sync).toEqual({ kind: "live", mode: "replay" });
  });

  // spec: PRT-020
  test("a replay that does not continue the position asks for a snapshot", () => {
    const events = fixture("text_reply");
    const answers: SubscribeResult[] = [
      { mode: "replay", epoch: "e2", fromSeq: 6 },
      { mode: "replay", epoch: "e1", fromSeq: 9 },
    ];
    for (const answer of answers) {
      const model = following("e1", events.slice(0, 5));
      model.connectionLost();

      const result = model.applySubscribed(TOPIC, answer);

      expect(result.ok === false && result.error.kind).toBe("replay_mismatch");
      expect(model.subscribeParams(TOPIC)?.since).toBeUndefined();
    }
  });

  // spec: PRT-022
  test("duplicates are ignored and a gap drops the position", () => {
    const events = fixture("text_reply");
    const model = following("e1", events.slice(0, 3));
    const before = view(model).state;

    expect(model.applyEvent(threadFrame(THREAD, "e1", 3, eventAt(events, 2)))).toBe("ignored");
    expect(view(model).state).toEqual(before);

    expect(model.applyEvent(threadFrame(THREAD, "e1", 5, eventAt(events, 4)))).toBe("gap");
    expect(view(model).state).toEqual(before);
    expect(model.topic(TOPIC)?.sync).toEqual({ kind: "subscribing" });
    expect(model.subscribeParams(TOPIC)?.since).toBeUndefined();
  });

  // spec: PRT-022
  test("a new epoch continues at seq one and anything else is a gap", () => {
    const events = fixture("restart_with_history");
    const model = following("e1", events.slice(0, 4));

    // A new omp process restarts numbering; the daemon's state carries over.
    applyAll(model, "e2", 1, events.slice(4));
    expect(view(model).state).toEqual(replay(events));

    const jumped = following("e1", events.slice(0, 4));
    expect(jumped.applyEvent(threadFrame(THREAD, "e2", 3, eventAt(events, 4)))).toBe("gap");
  });

  // spec: PRT-023
  test("resync keeps the data until the snapshot replaces it", () => {
    const events = fixture("text_reply");
    const model = following("e1", events.slice(0, 10));
    const stale = view(model).state;

    expect(model.resync(TOPIC)).toBe(true);

    expect(view(model).state).toEqual(stale);
    expect(model.applyEvent(threadFrame(THREAD, "e1", 11, eventAt(events, 10)))).toBe("ignored");
    expect(model.subscribeParams(TOPIC)?.since).toBeUndefined();
    const fresh = replay(events);
    expect(model.applySubscribed(TOPIC, snapshot("e1", events.length, fresh))).toEqual({ ok: true, mode: "snapshot" });
    expect(view(model).state).toEqual(fresh);
    // Untracked topics are not resynced.
    expect(model.resync(INDEX_TOPIC)).toBe(false);
  });

  test("the index is keyed and ordered by id and follows its events", () => {
    const model = new Model();
    model.track(INDEX_TOPIC);
    const state = indexState({
      workspaces: [workspace("01W2"), workspace("01W1")],
      threads: [summary("01T2", "second"), summary("01T1", "first")],
    });
    expect(model.applySubscribed(INDEX_TOPIC, snapshot("i1", 7, state)).ok).toBe(true);

    const renamed = summary("01T1", "renamed");
    const frames: EventFrame[] = [
      indexFrame("i1", 8, { kind: "thread_upserted", thread: renamed }),
      indexFrame("i1", 9, { kind: "thread_upserted", thread: summary("01T0", "zeroth") }),
      indexFrame("i1", 10, { kind: "thread_removed", threadId: "01T2" }),
      indexFrame("i1", 11, { kind: "workspace_removed", workspaceId: "01W2" }),
    ];
    for (const frame of frames) {
      expect(model.applyEvent(frame)).toBe("applied");
    }

    const { index } = model;
    expect([...index.threads.keys()]).toEqual(["01T0", "01T1"]);
    expect(index.threads.get("01T1")).toEqual(renamed);
    expect([...index.workspaces.keys()]).toEqual(["01W1"]);
  });

  // spec: DMN-075
  test("the index carries the user's theme and follows theme changes", () => {
    const model = new Model();
    expect(model.index.theme).toEqual({ css: null });
    model.track(INDEX_TOPIC);
    const themed = indexState({ theme: { css: ":root { --omfp-primary: red; }" } });
    expect(model.applySubscribed(INDEX_TOPIC, snapshot("i1", 1, themed)).ok).toBe(true);
    expect(model.index.theme).toEqual({ css: ":root { --omfp-primary: red; }" });
    model.takeDirty();

    const cleared = indexFrame("i1", 2, { kind: "theme_changed", theme: { css: null } });
    expect(model.applyEvent(cleared)).toBe("applied");

    expect(model.index.theme).toEqual({ css: null });
    expect(model.takeDirty().index).toBe(true);
  });

  // spec: DMN-075
  test("a snapshot without a theme has the built-in themes only", () => {
    const model = new Model();
    model.track(INDEX_TOPIC);
    const { theme: _, ...older } = indexState({ threads: [summary("01T1", "one")] });
    expect(model.applySubscribed(INDEX_TOPIC, snapshot("i1", 1, older)).ok).toBe(true);
    expect(model.index.theme).toEqual({ css: null });
    expect(model.index.threads.size).toBe(1);

    const broken = { ...indexState(), theme: { css: 42 } };
    expect(model.applySubscribed(INDEX_TOPIC, snapshot("i2", 1, broken)).ok).toBe(false);
  });

  // spec: DMN-009, UI-097
  test("the index carries the UI settings and follows settings changes", () => {
    const model = new Model();
    expect(model.index.settings).toEqual({ ui: {}, keymap: [], error: null });
    model.track(INDEX_TOPIC);
    const { settings: _, ...older } = indexState();
    expect(model.applySubscribed(INDEX_TOPIC, snapshot("i1", 1, older)).ok).toBe(true);
    expect(model.index.settings).toEqual({ ui: {}, keymap: [], error: null });
    model.takeDirty();

    const settings = {
      ui: { density: "normal", fonts: { prose: "Literata, serif" } },
      keymap: [{ mode: "normal", keys: "Space t", command: "theme.set" }],
      error: "config.toml: unknown field `densty`",
    } as const;
    const changed = indexFrame("i1", 2, {
      kind: "settings_changed",
      settings: { ...settings, keymap: [...settings.keymap] },
    });
    expect(model.applyEvent(changed)).toBe("applied");

    expect(model.index.settings).toEqual({ ...settings, keymap: [...settings.keymap] });
    expect(model.takeDirty().index).toBe(true);
    const broken = { ...indexState(), settings: { ui: {}, keymap: [{ keys: 1 }], error: null } };
    expect(model.applySubscribed(INDEX_TOPIC, snapshot("i2", 1, broken)).ok).toBe(false);
  });

  test("a snapshot replaces the previous state and position", () => {
    const model = new Model();
    model.track(INDEX_TOPIC);
    const first = indexState({ workspaces: [workspace("01W1")], threads: [summary("01T1", "one")] });
    model.applySubscribed(INDEX_TOPIC, snapshot("i1", 3, first));
    const second = indexState({ threads: [summary("01T9", "nine")] });

    model.applySubscribed(INDEX_TOPIC, snapshot("i2", 1, second));

    expect([...model.index.threads.keys()]).toEqual(["01T9"]);
    expect(model.index.workspaces.size).toBe(0);
    expect(model.topic(INDEX_TOPIC)?.position).toEqual({ epoch: "i2", seq: 1 });
  });

  test("events of unsubscribed topics or before the snapshot are ignored", () => {
    const model = new Model();
    const event: ThreadEvent = { kind: "process", state: { state: "starting" } };
    expect(model.applyEvent(threadFrame(THREAD, "e1", 1, event))).toBe("ignored");

    model.track(TOPIC);
    expect(model.applyEvent(threadFrame(THREAD, "e1", 1, event))).toBe("ignored");
    expect(model.thread(THREAD)).toBeUndefined();
  });

  test("an invalid snapshot fails the topic", () => {
    const model = new Model();
    model.track(TOPIC);

    const result = model.applySubscribed(TOPIC, snapshot("e1", 1, { nope: true }));

    expect(result.ok === false && result.error.kind).toBe("snapshot");
    const sync = model.topic(TOPIC)?.sync;
    expect(sync?.kind === "failed" && sync.error.code).toBe("internal");
    // A failed topic is subscribed again.
    expect(model.track(TOPIC)).toBe(true);
  });

  test("unsubscribing drops the topic and its data", () => {
    const model = following("e1", fixture("text_reply"));
    model.takeDirty();

    expect(model.untrack(TOPIC)).toBe(true);

    expect(model.thread(THREAD)).toBeUndefined();
    expect(model.topics).toEqual([]);
    const dirty = model.takeDirty();
    expect(dirty.topics).toBe(true);
    expect([...dirty.threads]).toEqual([THREAD]);
    expect(model.untrack(TOPIC)).toBe(false);
  });

  test("workspace topics keep the git status of each repository", () => {
    const model = new Model();
    const topic = "workspace/01W";
    model.track(topic);

    expect(model.applySubscribed(topic, snapshot("w1", 0, [])).ok).toBe(true);
    expect(model.workspaces.get("01W")).toEqual([]);

    const frame: EventFrame = {
      type: "event",
      topic,
      epoch: "w1",
      seq: 1,
      event: {
        kind: "status",
        repos: [{ name: "app", status: { ahead: 1, behind: 0, dirty: false, files: [] } }],
      },
    };
    expect(model.applyEvent(frame)).toBe("applied");
    const repos = model.workspaces.get("01W") ?? [];
    expect(repos.map((repo) => [repo.name, repo.status.ahead])).toEqual([["app", 1]]);
  });

  // spec: UI-012
  test("changes accumulate in one dirty set until drained", () => {
    const events = fixture("text_reply");
    const model = following("e1", events.slice(0, 1));
    model.track(INDEX_TOPIC);
    model.takeDirty();
    const revision = model.revision;

    applyAll(model, "e1", 2, events.slice(1));
    const state = indexState();
    model.applySubscribed(INDEX_TOPIC, snapshot("i1", 0, state));

    expect(model.revision).toBeGreaterThan(revision);
    const expected: Dirty = {
      connection: false,
      topics: true,
      index: true,
      threads: new Set([THREAD]),
      workspaces: new Set(),
    };
    expect(model.takeDirty()).toEqual(expected);
    expect(isDirtyEmpty(model.takeDirty())).toBe(true);
  });

  test("interactions closed by live events are kept with their resolution", () => {
    for (const name of ["approval_granted", "ask_answered"]) {
      const events = fixture(name);
      const model = following("e1", []);
      applyAll(model, "e1", 1, events);

      const thread = view(model);
      const answered = thread.closed.filter((closed) => closed.resolution !== undefined);
      const resolutions = events.filter((event) => event.kind === "interaction_resolved");
      expect({ name, answered: answered.length }).toEqual({ name, answered: resolutions.length });
      const client = resolutions[0]?.kind === "interaction_resolved" ? resolutions[0].resolution.client : undefined;
      expect(client).toBeDefined();
      for (const closed of answered) {
        expect(closed.resolution).toEqual({ by: "client", client: client ?? "" });
      }
      expect(thread.state.interactions).toEqual([]);
    }
  });

  test("a snapshot restarts the closed interaction history", () => {
    const events = fixture("approval_granted");
    const model = following("e1", []);
    applyAll(model, "e1", 1, events);
    expect(view(model).closed).not.toEqual([]);

    model.applySubscribed(TOPIC, snapshot("e2", 1, replay(events)));

    expect(view(model).closed).toEqual([]);
  });
});
