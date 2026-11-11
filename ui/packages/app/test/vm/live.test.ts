/** L1: which live voice path a window takes (UI-188). */

import { describe, expect, test } from "bun:test";
import { liveSupport } from "../../src/vm/live.ts";

describe("live support", () => {
  // spec: UI-188, UI-180
  test("WebRTC where it works, omp's own session on the daemon's machine, else why not", () => {
    const at = (secure: boolean, webrtc: boolean, local: boolean) => liveSupport({ secure, webrtc, local });
    // A browser on localhost or HTTPS, wherever the daemon runs.
    expect(at(true, true, false)).toBe("webrtc");
    expect(at(true, true, true)).toBe("webrtc");
    // The desktop webview (no WebRTC) on a local daemon.
    expect(at(true, false, true)).toBe("host");
    expect(at(false, false, true)).toBe("host");
    expect(at(false, true, true)).toBe("host");
    // The desktop webview on a remote daemon (`--connect`), or a browser without WebRTC.
    expect(at(true, false, false)).toBe("unsupported");
    expect(at(false, false, false)).toBe("unsupported");
    // A browser on a LAN address.
    expect(at(false, true, false)).toBe("insecure");
  });
});
