// Run by `just check-workflow` from the repository root (`bun test ./.github`).
import { describe, expect, test } from "bun:test";
import { check, type Inputs, read, type Workflow } from "./check-workflow.ts";

const repository = await read(process.env.JUST ?? "just");

/** The repository's inputs with `ci.yml` changed by `edit` (on a copy). */
function withWorkflow(edit: (workflow: Workflow) => void): Inputs {
  const workflow = structuredClone(repository.workflow);
  edit(workflow);
  return { ...repository, workflow };
}

describe("check-workflow", () => {
  // spec: TST-112
  test("the repository's workflows pass", () => {
    expect(check(repository)).toEqual([]);
  });

  // spec: TST-112
  test("removing the Linux desktop job fails, though the macOS job runs the recipe on pushes", () => {
    const problems = check(
      withWorkflow((workflow) => {
        delete workflow.jobs["test-desktop"];
      }),
    );
    expect(problems).toContain(
      "`just ci` runs test-desktop, no job of .github/workflows/ci.yml does on every run (pull requests too)",
    );
  });

  // spec: TST-112
  test("removing the Linux package job fails", () => {
    const problems = check(
      withWorkflow((workflow) => {
        delete workflow.jobs.package;
      }),
    );
    expect(problems).toContain(
      "`just ci` runs package, no job of .github/workflows/ci.yml does on every run (pull requests too)",
    );
  });

  // spec: TST-112
  test("a job running a recipe `just ci` does not fails", () => {
    const problems = check(
      withWorkflow((workflow) => {
        workflow.jobs.check?.steps?.push({ run: "nix develop -c just serve" });
      }),
    );
    expect(problems).toContain("job check runs `just serve`, which `just ci` does not");
  });

  // spec: TST-112
  test("every job running the desktop tests serves the shared bundle, the Linux one too", () => {
    const problems = check(
      withWorkflow((workflow) => {
        const job = workflow.jobs["test-desktop"];
        if (job !== undefined) delete job.env;
      }),
    );
    expect(problems).toContain("job test-desktop must serve the bundle of job web through OMFP_WEB_BUNDLE");
  });
});
