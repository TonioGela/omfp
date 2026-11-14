import { expect, test } from "bun:test";
import { copyFile, mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

async function notes(changelog: string, tag: string) {
  const root = await mkdtemp(join(tmpdir(), "omfp-release-notes-"));
  try {
    const scripts = join(root, "contrib", "release");
    await mkdir(scripts, { recursive: true });
    const script = join(scripts, "notes.sh");
    await copyFile(new URL("../contrib/release/notes.sh", import.meta.url), script);
    await writeFile(join(root, "CHANGELOG.md"), changelog);
    const result = Bun.spawnSync(["bash", script, tag], { cwd: tmpdir() });
    return {
      code: result.exitCode,
      stdout: result.stdout.toString(),
      stderr: result.stderr.toString(),
    };
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

// spec: ENV-060
test("release notes include only the exact tagged version, preserving Markdown", async () => {
  const body = "\n### Added\n\n- Open **PDFs**.\n\n### Fixed\n\n- Keep unsaved edits.\n\n";
  const result = await notes(
    `# Changelog\n\n## Unreleased\n\n- Future work.\n\n## 0.1.0 — 2026-11-14\n${body}## 0.0.9 — 2026-11-01\n\n- Older work.\n`,
    "v0.1.0",
  );
  expect(result.code).toBe(0);
  expect(result.stdout).toBe(body);
});

// spec: ENV-060
test("a prerelease at EOF works without a final newline or git history", async () => {
  const result = await notes("# Changelog\n\n## 0.2.0-rc.1 — 2026-11-14\n\n- Preview.", "v0.2.0-rc.1");
  expect(result.code).toBe(0);
  expect(result.stdout).toBe("\n- Preview.\n");
});

// spec: ENV-060
test("missing, undated and empty releases fail without publishing notes", async () => {
  for (const section of [
    "## Unreleased\n\n- Future work.\n\n## 0.1.01 — 2026-11-14\n\n- Not this version.\n",
    "## 0.1.0\n\n- Undated.\n",
    "## 0.1.0 — 2026-11-14\n\n### Added\n\n## 0.0.9 — 2026-11-01\n\n- Old work.\n",
  ]) {
    const result = await notes(`# Changelog\n\n${section}`, "v0.1.0");
    expect(result.code).not.toBe(0);
    expect(result.stdout).toBe("");
    expect(result.stderr).toContain("v0.1.0");
  }
});
