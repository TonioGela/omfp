/** L4: the per-frame snapshot store (UI-012). */

import { describe, expect, test } from "bun:test";
import { FrameStore } from "../../src/sdk/index.ts";
import { FakeClient, ManualFrames } from "../support/host.tsx";

/** Lets pending promise continuations run (no timers involved). */
async function settle(): Promise<void> {
  for (let turn = 0; turn < 100; turn += 1) {
    await Promise.resolve();
  }
}

describe("frame store", () => {
  // spec: UI-012
  test("any number of model changes between two frames wake subscribers once, with one snapshot", async () => {
    const client = new FakeClient();
    const frames = new ManualFrames();
    const store = new FrameStore(client, frames.schedule);
    let wakes = 0;
    store.subscribe(() => {
      wakes += 1;
    });
    store.start();

    client.connected();
    client.index({});
    client.indexEvents({ kind: "workspace_removed", workspaceId: "01W" });
    await settle();
    expect(frames.pending).toBe(1);
    expect(wakes).toBe(0);

    frames.run();
    expect(wakes).toBe(1);
    const snapshot = store.getSnapshot();
    expect(snapshot.frame).toBe(1);
    expect(snapshot.connection.kind).toBe("connected");
    expect(snapshot.dirty.index).toBe(true);
    expect(snapshot.dirty.connection).toBe(true);

    await settle();
    expect(frames.pending).toBe(0);
    store.stop();
  });

  // spec: UI-012
  test("the snapshot is stable between frames, so selectors compare by identity", async () => {
    const client = new FakeClient();
    const frames = new ManualFrames();
    const store = new FrameStore(client, frames.schedule);
    store.start();
    const before = store.getSnapshot();

    client.index({});
    expect(store.getSnapshot()).toBe(before);
    await settle();
    frames.run();
    const after = store.getSnapshot();
    expect(after).not.toBe(before);
    expect(after.index).toBe(client.model.index);
    store.commit();
    expect(store.getSnapshot()).toBe(after);
    store.stop();
  });
});
