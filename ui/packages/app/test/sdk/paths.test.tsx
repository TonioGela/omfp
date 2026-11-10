/** L4: the path field's sources go to the daemon (DMN-021). */

import { afterEach, describe, expect, test } from "bun:test";
import { cleanup } from "@testing-library/react";
import { type PathSources, usePathSources } from "../../src/sdk/paths.ts";
import { renderWithHost } from "../support/host.tsx";

afterEach(cleanup);

function Probe({ rendered }: { rendered: PathSources[] }) {
  rendered.push(usePathSources());
  return null;
}

describe("usePathSources", () => {
  // spec: DMN-021, UI-023, UI-025
  test("suggests with repos.suggest and completes with fs.complete", async () => {
    const rendered: PathSources[] = [];
    const { client } = renderWithHost(<Probe rendered={rendered} />);
    client.answers.set("repos.suggest", () => ({
      repos: [{ path: "/home/u/src/omfp", name: "omfp", lastSeenAt: 2 }],
    }));
    client.answers.set("fs.complete", () => ({
      entries: [{ name: "omfp", path: "/home/u/src/omfp", isGitRoot: true }],
    }));
    const sources = rendered.at(-1);
    if (sources === undefined) throw new Error("the probe did not render");

    expect((await sources.suggest("omf")).map(({ path, name }) => ({ path, name }))).toEqual([
      { path: "/home/u/src/omfp", name: "omfp" },
    ]);
    expect(await sources.complete("~/src/om")).toEqual([{ name: "omfp", path: "/home/u/src/omfp", isGitRoot: true }]);
    expect(client.callsTo("repos.suggest").map((call) => call.params)).toEqual([{ query: "omf" }]);
    expect(client.callsTo("fs.complete").map((call) => call.params)).toEqual([{ path: "~/src/om" }]);
  });
});
