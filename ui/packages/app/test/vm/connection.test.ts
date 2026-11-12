/** L1: the connection banner, the live-region status line and the access token (ports of the Dioxus
 * `vm/connection.rs` and `host/token.rs` tests). */

import { describe, expect, test } from "bun:test";
import type { ConnectionStatus } from "@omfp/client";
import { announcement, banner } from "../../src/vm/connection.ts";
import {
  chooseToken,
  fragmentLocal,
  fragmentTicket,
  fragmentToken,
  fragmentWithoutCredential,
  rejection,
  TOKEN_KEY,
  type TokenPage,
  takeCredential,
  takeThread,
} from "../../src/vm/token.ts";
import { thread } from "../support/fixtures.ts";

const connected: ConnectionStatus = {
  kind: "connected",
  hello: {
    protocol: 1,
    daemon: "0.1.0",
    omp: "18.18.6",
    warnings: [],
    widgetBase: "/widgets/test",
    connection: 1,
  },
};

describe("connection", () => {
  // spec: UI-003
  test("the banner shows reconnect progress and disappears once connected", () => {
    expect(banner({ kind: "reconnecting", attempt: 3, retryInMs: 4200, error: "closed" })).toEqual({
      tone: "warning",
      text: "Connection lost. Reconnecting in 5 s (attempt 3)…",
    });
    expect(banner(connected)).toBeUndefined();
    expect(banner({ kind: "connecting" })?.tone).toBe("info");
    expect(banner({ kind: "unauthenticated", message: "invalid token" })?.tone).toBe("error");
    expect(banner({ kind: "version_mismatch", supported: [2], message: "wants 2" })?.tone).toBe("error");
  });

  // spec: UI-092
  test("the announcement names the selected thread's state and the waiting threads", () => {
    const threads = [
      thread("t1", "checkout", "w", { needsAttention: true }),
      thread("t2", "flaky", "w", { needsAttention: true }),
    ];
    expect(announcement(connected, threads, "t1")).toBe("checkout: awaiting input. 2 threads await input");
    expect(announcement({ kind: "reconnecting", attempt: 1, retryInMs: 1000, error: "" }, [], undefined)).toBe(
      "Connection lost. Reconnecting in 1 s (attempt 1)…",
    );
    expect(announcement(connected, threads.slice(1), undefined)).toBe("1 thread awaits input");
  });
});

describe("token", () => {
  // spec: UI-002
  test("the token is read from the fragment and removed from it", () => {
    expect(fragmentToken("#token=abc123")).toBe("abc123");
    expect(fragmentWithoutCredential("#token=abc123")).toBe("");
    expect(fragmentToken("#thread=t1&token=abc123")).toBe("abc123");
    expect(fragmentWithoutCredential("#thread=t1&token=abc123")).toBe("#thread=t1");
    expect(fragmentToken("#token=")).toBeUndefined();
    expect(fragmentToken("")).toBeUndefined();
    expect(fragmentWithoutCredential("")).toBe("");
    expect(fragmentToken("#tokens=abc")).toBeUndefined();
    expect(fragmentWithoutCredential("#tokens=abc")).toBe("#tokens=abc");
  });

  // spec: UI-002
  test("a token from the address bar replaces the stored one", () => {
    expect(chooseToken("new", "old")).toBe("new");
    expect(chooseToken(undefined, "old")).toBe("old");
    expect(chooseToken(undefined, "  ")).toBeUndefined();
    expect(chooseToken(undefined, null)).toBeUndefined();
  });

  // spec: UI-002
  test("taking the token stores it and replaces the address without the fragment", () => {
    const stored = new Map<string, string>();
    const replaced: string[] = [];
    const page = (hash: string): TokenPage => ({
      location: { hash, pathname: "/", search: "?x=1" },
      history: { replaceState: (_data, _unused, url) => replaced.push(String(url)) },
      storage: { getItem: (key) => stored.get(key) ?? null, setItem: (key, value) => stored.set(key, value) },
    });
    expect(takeCredential(page("#thread=t1&token=abc"))).toEqual({ token: "abc" });
    expect(stored.get(TOKEN_KEY)).toBe("abc");
    expect(replaced).toEqual(["/?x=1#thread=t1"]);
    expect(takeCredential(page(""))).toEqual({ token: "abc" });
    expect(replaced).toHaveLength(1);
    stored.clear();
    expect(takeCredential(page(""))).toBeUndefined();
  });

  // spec: UI-255
  test("a ticket is taken from the fragment, removed from it and never stored", () => {
    expect(fragmentTicket("#ticket=b41d07&local=1")).toBe("b41d07");
    expect(fragmentTicket("#ticket=")).toBeUndefined();
    expect(fragmentTicket("#token=abc")).toBeUndefined();
    expect(fragmentWithoutCredential("#ticket=b41d07&thread=t1")).toBe("#thread=t1");
    const stored = new Map<string, string>([[TOKEN_KEY, "old"]]);
    const replaced: string[] = [];
    const page: TokenPage = {
      location: { hash: "#ticket=b41d07&thread=t1", pathname: "/", search: "" },
      history: { replaceState: (_data, _unused, url) => replaced.push(String(url)) },
      storage: { getItem: (key) => stored.get(key) ?? null, setItem: (key, value) => stored.set(key, value) },
    };
    // A fresh link wins over the stored token, which stays until the ticket's token replaces it.
    expect(takeCredential(page)).toEqual({ ticket: "b41d07" });
    expect(replaced).toEqual(["/#thread=t1"]);
    expect([...stored.entries()]).toEqual([[TOKEN_KEY, "old"]]);
  });

  // spec: UI-255
  test("a refused ticket says whether it expired or was already used", () => {
    expect(rejection({ token: "t" }, "invalid token")).toBe("The daemon rejected this token.");
    expect(rejection({ ticket: "b" }, "ticket expired")).toContain("expired");
    expect(rejection({ ticket: "b" }, "invalid ticket")).toContain("expired");
    const used = rejection({ ticket: "b" }, "ticket already used");
    expect(used).toContain("already used");
    expect(used).toContain("someone else");
  });

  // spec: UI-188
  test("`local=1` marks a window on the daemon's machine; it stays in the address bar and is never stored", () => {
    expect(fragmentLocal("#token=abc&local=1")).toBe(true);
    expect(fragmentLocal("#local=1")).toBe(true);
    expect(fragmentLocal("#token=abc")).toBe(false);
    expect(fragmentLocal("#local=0")).toBe(false);
    expect(fragmentLocal("#local=10")).toBe(false);
    const stored = new Map<string, string>();
    const replaced: string[] = [];
    const page: TokenPage = {
      location: { hash: "#token=abc&local=1", pathname: "/", search: "" },
      history: { replaceState: (_data, _unused, url) => replaced.push(String(url)) },
      storage: { getItem: (key) => stored.get(key) ?? null, setItem: (key, value) => stored.set(key, value) },
    };
    expect(takeCredential(page)).toEqual({ token: "abc" });
    expect(replaced).toEqual(["/#local=1"]);
    expect([...stored.keys()]).toEqual([TOKEN_KEY]);
  });

  // spec: UI-218
  test("`thread=<id>` selects a thread once: it is read and removed from the address bar", () => {
    const replaced: string[] = [];
    const page = (hash: string) => ({
      location: { hash, pathname: "/", search: "?x=1" },
      history: {
        replaceState: (_data: unknown, _unused: string, url?: string | URL | null) => replaced.push(String(url)),
      },
    });
    expect(takeThread(page("#local=1&thread=t1"))).toBe("t1");
    expect(replaced).toEqual(["/?x=1#local=1"]);
    expect(takeThread(page("#thread=a%26b%3Dc"))).toBe("a&b=c");
    expect(replaced[1]).toBe("/?x=1");
    // Nothing to take: the address stays.
    expect(takeThread(page("#local=1"))).toBeUndefined();
    expect(takeThread(page("#threads=t1"))).toBeUndefined();
    expect(replaced).toHaveLength(2);
    // Empty or malformed: removed, nothing selected.
    expect(takeThread(page("#thread="))).toBeUndefined();
    expect(takeThread(page("#thread=%E0%A4%A"))).toBeUndefined();
    expect(replaced).toHaveLength(4);
  });
});
