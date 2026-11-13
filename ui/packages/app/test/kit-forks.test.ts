/** L0: the app renders the kit's components; it never writes a copy of their markup (UI-440). */

import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const SRC = join(import.meta.dir, "../src");

/** Base classes only a kit component writes (with their `__part` classes): an app file writing one copies it. */
const KIT_ONLY = [
  "omfp-diffstat",
  "omfp-todo",
  "omfp-queue",
  "omfp-queue-chip",
  "omfp-modeline",
  "omfp-mode-pill",
  "omfp-chat-header",
];

/** `file: class` for every app source file writing a kit-only class. */
function forks(): string[] {
  return [...new Bun.Glob("**/*.{ts,tsx}").scanSync(SRC)].sort().flatMap((file) => {
    const source = readFileSync(join(SRC, file), "utf8");
    return KIT_ONLY.filter((name) => new RegExp(`(?<![\\w-])${name}(?:__[\\w-]+)?(?![\\w-])`).test(source)).map(
      (name) => `${file}: ${name}`,
    );
  });
}

describe("kit components", () => {
  // spec: TST-109, UI-440
  test("no app file hand-builds the markup of a kit component", () => {
    expect(forks()).toEqual([]);
  });
});
