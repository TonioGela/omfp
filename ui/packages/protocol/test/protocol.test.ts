import { describe, expect, test } from "bun:test";
import {
  formatTopic,
  INDEX_TOPIC,
  METHODS,
  type MethodName,
  type ParamsOf,
  PROTOCOL_VERSION,
  parseTopic,
  type ResultOf,
  request,
  threadTopic,
  workspaceTopic,
} from "../src/index.ts";

describe("topics", () => {
  test("round trip through their names", () => {
    const cases = [
      [{ kind: "index" }, "index"],
      [{ kind: "thread", id: "01J0THREAD" }, "thread/01J0THREAD"],
      [{ kind: "workspace", id: "01J0WS" }, "workspace/01J0WS"],
      [{ kind: "terminal", id: "term_1-a" }, "terminal/term_1-a"],
    ] as const;
    for (const [topic, name] of cases) {
      expect(formatTopic(topic)).toBe(name);
      expect(parseTopic(name)).toEqual(topic);
    }
    expect(INDEX_TOPIC).toBe("index");
    expect(threadTopic("01J0THREAD")).toBe("thread/01J0THREAD");
    expect(workspaceTopic("01J0WS")).toBe("workspace/01J0WS");
  });

  test("malformed names are rejected", () => {
    const names = [
      "",
      "index/",
      "index/1",
      "thread",
      "thread/",
      "thread/a/b",
      "thread/a b",
      "Thread/01J",
      "threads/01J",
      "repo/01J",
      "/01J",
      " index",
    ];
    for (const name of names) {
      expect(parseTopic(name)).toBeUndefined();
    }
  });
});

describe("method table", () => {
  // spec: AGT-007
  test("lists every method once and types params and results", () => {
    expect(new Set(METHODS).size).toBe(METHODS.length);
    expect(METHODS).toContain("system.hello");
    expect(METHODS).toContain("thread.prompt");

    const hello: ParamsOf<"system.hello"> = { token: "t", client: "omfp/test", protocol: PROTOCOL_VERSION };
    const frame = request("1", "system.hello", hello);
    expect(frame).toEqual({ type: "request", id: "1", method: "system.hello", params: hello });

    const result: ResultOf<"system.hello"> = { daemon: "0.1.0", omp: "18.18.6", protocol: 1, warnings: [] };
    expect(result.protocol).toBe(PROTOCOL_VERSION);

    // @ts-expect-error: `text` is required by `thread.prompt`.
    const missing: ParamsOf<"thread.prompt"> = { threadId: "01J" };
    expect(missing.threadId).toBe("01J");
    const name: MethodName = "subscribe";
    expect(METHODS).toContain(name);
  });
});
