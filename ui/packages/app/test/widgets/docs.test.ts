/** L0: the agents' widget reference (`omfp://docs/widgets`) lists only names `omfp/sdk` and `omfp/kit` export. */

import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import * as kit from "omfp/kit";
import * as sdk from "omfp/sdk";

const DOCS = readFileSync(join(import.meta.dir, "../../../../../crates/omfp-daemon/src/widgets/docs.md"), "utf8");

/** The identifiers of the first column of the table under `## <heading>` (`useJob(command)` → `useJob`). */
function documented(heading: string): string[] {
  const start = DOCS.indexOf(`\n## ${heading}\n`);
  expect(start).toBeGreaterThanOrEqual(0);
  const end = DOCS.indexOf("\n## ", start + 1);
  const section = DOCS.slice(start, end < 0 ? undefined : end);
  return section
    .split("\n")
    .filter((line) => line.startsWith("| `"))
    .map((line) => /^\| `([A-Za-z_$][\w$]*)/.exec(line)?.[1] ?? line);
}

describe("widget docs", () => {
  // spec: AGT-016, AGT-025
  test("every name the omfp/sdk table lists is exported by omfp/sdk", () => {
    const names = documented("omfp/sdk");
    expect(names.length).toBeGreaterThan(10);
    expect(names.filter((name) => !(name in sdk))).toEqual([]);
  });

  // spec: AGT-017, AGT-025
  test("every name the omfp/kit table lists is exported by omfp/kit", () => {
    const names = documented("omfp/kit");
    expect(names.length).toBeGreaterThan(10);
    expect(names.filter((name) => !(name in kit))).toEqual([]);
  });
});
