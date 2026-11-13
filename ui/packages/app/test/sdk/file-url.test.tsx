/** L4: `useFileUrl`, the URL of a workspace file's raw bytes (UI-540, DMN-250). */

import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen } from "@testing-library/react";
import { fileUrl } from "../../src/sdk/files.ts";
import { OmfpProvider, SELECTED_THREAD, useFileUrl } from "../../src/sdk/index.ts";
import { thread, workspace } from "../support/fixtures.ts";
import { testHost } from "../support/host.tsx";

afterEach(cleanup);

function Url({ path }: { readonly path: string }) {
  return <span data-testid="url">{useFileUrl(path) ?? "none"}</span>;
}

function url(): string | null {
  return screen.getByTestId("url").textContent;
}

describe("useFileUrl", () => {
  // spec: UI-540
  test("is the file base of hello, the current workspace and the path with each segment encoded", () => {
    expect(fileUrl("/files/k", "w 1", "docs/a b#1.mp4")).toBe("/files/k/w%201/docs/a%20b%231.mp4");
    // An absolute path keeps its leading slash; the daemon refuses one outside the workspace folder.
    expect(fileUrl("/files/k", "w1", "/home/me/ws/clip.webm")).toBe("/files/k/w1//home/me/ws/clip.webm");
  });

  // spec: UI-540
  test("is undefined before hello and without a workspace, and keeps the last base while reconnecting", () => {
    const test = testHost();
    render(
      <OmfpProvider host={test.host}>
        <Url path="media/clip.webm" />
      </OmfpProvider>,
    );
    expect(url()).toBe("none");

    act(() => {
      test.client.connected();
      test.flush();
    });
    expect(url()).toBe("none");

    act(() => {
      test.client.index({ workspaces: [workspace("w1")], threads: [thread("t1", "one", "w1")] });
      test.host.window.set(SELECTED_THREAD, "t1");
      test.flush();
    });
    expect(url()).toBe("/files/test/w1/media/clip.webm");

    act(() => {
      test.client.model.setConnection({ kind: "reconnecting", attempt: 1, retryInMs: 500, error: "closed" });
      test.client.notify();
      test.flush();
    });
    expect(url()).toBe("/files/test/w1/media/clip.webm");
  });
});
