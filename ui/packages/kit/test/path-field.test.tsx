/** L4: the path field (suggestions and completion for typed paths, DMN-021) used by UI-023 and UI-025. */

import { afterEach, describe, expect, type Mock, mock, test } from "bun:test";
import { act, cleanup, configure, getConfig, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { type PathCompletion, PathField, type PathSuggestion } from "../src/index.ts";

afterEach(cleanup);

const SEEN: readonly PathSuggestion[] = [
  { path: "/home/u/src/omfp", name: "omfp" },
  { path: "/home/u/src/notes", name: "notes" },
];

/** A fake daemon host: directories by parent, answered as `fs.complete` does (DMN-021). */
const TREE: Readonly<Record<string, readonly { name: string; isGitRoot: boolean }[]>> = {
  "/": [
    { name: "abc", isGitRoot: false },
    { name: "home", isGitRoot: false },
  ],
  "/home/u/": [
    { name: "projects", isGitRoot: false },
    { name: "prototypes", isGitRoot: false },
    { name: "src", isGitRoot: false },
  ],
  "/home/u/projects/": [
    { name: "api", isGitRoot: true },
    { name: "archive", isGitRoot: false },
  ],
};

function fakeComplete(path: string): readonly PathCompletion[] {
  const cut = path.lastIndexOf("/") + 1;
  const parent = path.slice(0, cut);
  const segment = path.slice(cut);
  return (TREE[parent] ?? [])
    .filter((entry) => entry.name.startsWith(segment))
    .map((entry) => ({ ...entry, path: `${parent}${entry.name}` }));
}

interface Sources {
  readonly suggest: Mock<(query: string) => Promise<readonly PathSuggestion[]>>;
  readonly complete: Mock<(path: string) => Promise<readonly PathCompletion[]>>;
}

function sources(): Sources {
  return {
    suggest: mock(
      async (query: string): Promise<readonly PathSuggestion[]> =>
        SEEN.filter((repo) => repo.path.includes(query) || repo.name.includes(query)),
    ),
    complete: mock(async (path: string): Promise<readonly PathCompletion[]> => fakeComplete(path)),
  };
}

function Harness({
  initial = "",
  source,
  onSubmit,
  withSuggest = true,
}: {
  initial?: string;
  source: Pick<Sources, "complete"> & Partial<Pick<Sources, "suggest">>;
  onSubmit?: (value: string) => void;
  withSuggest?: boolean;
}) {
  const [value, setValue] = useState(initial);
  return (
    <>
      <PathField
        label="Folder"
        value={value}
        onChange={setValue}
        {...(onSubmit === undefined ? {} : { onSubmit })}
        {...(withSuggest && source.suggest !== undefined ? { suggest: source.suggest } : {})}
        complete={source.complete}
        data-testid="path"
      />
      <button type="button" data-testid="after">
        after
      </button>
    </>
  );
}

function input(): HTMLInputElement {
  const element = screen.getByTestId("path");
  if (!(element instanceof HTMLInputElement)) throw new Error("the path field's test id is not on its input");
  return element;
}

function shown(): { path: string | null; kind: string | null; git: boolean }[] {
  return screen.queryAllByTestId("path-option").map((option) => ({
    path: option.getAttribute("data-path"),
    kind: option.getAttribute("data-kind"),
    git: option.hasAttribute("data-git-root"),
  }));
}

function completedPaths(source: Sources): string[] {
  return source.complete.mock.calls.map(([path]) => path);
}

describe("PathField", () => {
  // spec: UI-023, UI-025, DMN-021
  test("lists the repositories seen first, then the directories of the typed path, git roots marked", async () => {
    const user = userEvent.setup();
    const source = sources();
    render(<Harness source={source} initial="/home/u/projects/" />);
    await user.click(input());
    await waitFor(() => expect(shown().length).toBe(2));
    // Nothing seen matches the projects folder yet: only directories.
    expect(shown()).toEqual([
      { path: "/home/u/projects/api", kind: "directory", git: true },
      { path: "/home/u/projects/archive", kind: "directory", git: false },
    ]);
    await user.clear(input());
    await user.type(input(), "/home/u/");
    await waitFor(() => expect(shown().length).toBe(5));
    expect(shown()).toEqual([
      { path: "/home/u/src/omfp", kind: "suggestion", git: false },
      { path: "/home/u/src/notes", kind: "suggestion", git: false },
      { path: "/home/u/projects", kind: "directory", git: false },
      { path: "/home/u/prototypes", kind: "directory", git: false },
      { path: "/home/u/src", kind: "directory", git: false },
    ]);
    expect(input().getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByTestId("path-options").getAttribute("role")).toBe("listbox");
  });

  // spec: UI-023, DMN-021
  test("asks for completions of the typed path only when it starts with / or ~", async () => {
    const user = userEvent.setup();
    const source = sources();
    render(<Harness source={source} />);
    await user.type(input(), "omf");
    await waitFor(() => expect(shown().map((option) => option.path)).toEqual(["/home/u/src/omfp"]));
    expect(completedPaths(source)).toEqual([]);
    expect(source.suggest.mock.calls.at(-1)).toEqual(["omf"]);
    await user.clear(input());
    await user.type(input(), "/a");
    await waitFor(() => expect(shown().map((option) => option.path)).toEqual(["/abc"]));
    expect(completedPaths(source).at(-1)).toBe("/a");
    expect(source.suggest.mock.calls.at(-1)).toEqual(["/a"]);
  });

  // spec: DMN-021
  test("ignores answers to a path that is no longer typed", async () => {
    const user = userEvent.setup();
    const pending = new Map<string, (entries: readonly PathCompletion[]) => void>();
    const complete = mock((path: string) => {
      const { promise, resolve } = Promise.withResolvers<readonly PathCompletion[]>();
      pending.set(path, resolve);
      return promise;
    });
    render(<Harness source={{ complete }} />);
    await user.type(input(), "/a");
    await act(async () => pending.get("/a")?.([{ name: "abc", path: "/abc", isGitRoot: false }]));
    await act(async () => pending.get("/")?.(fakeComplete("/")));
    expect(shown().map((option) => option.path)).toEqual(["/abc"]);
  });

  // spec: UI-023, DMN-021
  test("keeps every key of a long path typed in one burst while completions answer late and out of order", async () => {
    // A burst of key presses with no task in between, as a fast typist or WebDriver produces: no `act` around each
    // event (it would flush React's pending work after every key) and no delay between keys.
    const { eventWrapper } = getConfig();
    configure({ eventWrapper: (dispatch) => dispatch() });
    try {
      const user = userEvent.setup({ delay: null });
      const pending: { path: string; resolve: (entries: readonly PathCompletion[]) => void }[] = [];
      const complete = mock((path: string) => {
        const { promise, resolve } = Promise.withResolvers<readonly PathCompletion[]>();
        pending.push({ path, resolve });
        return promise;
      });
      render(<Harness source={{ complete }} />);
      const typed = "/tmp/nix-shell.qDOUxB/nix-shell.kPxRyg/omfp-shot-3jzRzU/projects/ap";
      await user.type(input(), typed);
      expect(input().value).toBe(typed);
      expect(complete.mock.calls.at(-1)).toEqual([typed]);
      // The answers arrive last typed first; only the one for the typed path is shown, and nothing changes the value.
      for (const { path, resolve } of pending.toReversed()) {
        await act(async () => resolve(fakeComplete(path === typed ? "/home/u/projects/" : "/")));
      }
      expect(input().value).toBe(typed);
      expect(shown().map((option) => option.path)).toEqual(["/home/u/projects/api", "/home/u/projects/archive"]);
    } finally {
      configure({ eventWrapper });
    }
  });

  // spec: UI-023, UI-025, DMN-021
  test("Tab completes the common prefix of the directories, then a single match with its slash", async () => {
    const user = userEvent.setup();
    const source = sources();
    render(<Harness source={source} initial="/home/u/pr" withSuggest={false} />);
    await user.click(input());
    await waitFor(() => expect(shown().length).toBe(2));
    await user.keyboard("{Tab}");
    expect(input().value).toBe("/home/u/pro");
    expect(document.activeElement).toBe(input());
    await user.type(input(), "j");
    await waitFor(() => expect(shown().map((option) => option.path)).toEqual(["/home/u/projects"]));
    await user.keyboard("{Tab}");
    expect(input().value).toBe("/home/u/projects/");
    expect(document.activeElement).toBe(input());
    await waitFor(() => expect(completedPaths(source).at(-1)).toBe("/home/u/projects/"));
  });

  // spec: UI-023
  test("Tab moves on when it has nothing to complete", async () => {
    const user = userEvent.setup();
    const source = sources();
    render(<Harness source={source} initial="/home/u/" withSuggest={false} />);
    await user.click(input());
    // `projects`, `prototypes` and `src` share no prefix.
    await waitFor(() => expect(shown().length).toBe(3));
    await user.keyboard("{Tab}");
    expect(input().value).toBe("/home/u/");
    expect(document.activeElement).toBe(screen.getByTestId("after"));
  });

  // spec: UI-023, UI-025, DMN-021
  test("picking a directory appends a slash and keeps the list open for its children", async () => {
    const user = userEvent.setup();
    const source = sources();
    render(<Harness source={source} initial="/home/u/" withSuggest={false} />);
    await user.click(input());
    await waitFor(() => expect(shown().length).toBe(3));
    const projects = screen
      .getAllByTestId("path-option")
      .find((option) => option.getAttribute("data-path") === "/home/u/projects");
    if (projects === undefined) throw new Error("no projects option");
    await user.click(projects);
    expect(input().value).toBe("/home/u/projects/");
    expect(document.activeElement).toBe(input());
    await waitFor(() =>
      expect(shown().map((option) => option.path)).toEqual(["/home/u/projects/api", "/home/u/projects/archive"]),
    );
  });

  // spec: UI-023, UI-025
  test("the arrows highlight an option; Enter on a git root takes its path and closes the list", async () => {
    const user = userEvent.setup();
    const onSubmit = mock((_value: string) => undefined);
    const source = sources();
    render(<Harness source={source} initial="/home/u/projects/" onSubmit={onSubmit} withSuggest={false} />);
    await user.click(input());
    await waitFor(() => expect(shown().length).toBe(2));
    await user.keyboard("{ArrowDown}{ArrowDown}{ArrowUp}");
    const [api] = screen.getAllByTestId("path-option");
    expect(api?.getAttribute("aria-selected")).toBe("true");
    expect(input().getAttribute("aria-activedescendant")).toBe(api?.id ?? "");
    await user.keyboard("{Enter}");
    expect(input().value).toBe("/home/u/projects/api");
    expect(shown()).toEqual([]);
    expect(input().getAttribute("aria-expanded")).toBe("false");
    expect(onSubmit).not.toHaveBeenCalled();
    // After accepting an option, Enter submits.
    await user.keyboard("{Enter}");
    expect(onSubmit.mock.calls).toEqual([["/home/u/projects/api"]]);
  });

  // spec: UI-023, UI-025
  test("clicking a suggestion takes its path and closes the list", async () => {
    const user = userEvent.setup();
    const source = sources();
    render(<Harness source={source} />);
    await user.type(input(), "notes");
    await waitFor(() => expect(shown().map((option) => option.kind)).toEqual(["suggestion"]));
    const [notes] = screen.getAllByTestId("path-option");
    if (notes === undefined) throw new Error("no suggestion");
    await user.click(notes);
    expect(input().value).toBe("/home/u/src/notes");
    expect(shown()).toEqual([]);
  });

  // spec: UI-023
  test("Enter without a highlighted option submits the typed value", async () => {
    const user = userEvent.setup();
    const onSubmit = mock((_value: string) => undefined);
    const source = sources();
    render(<Harness source={source} onSubmit={onSubmit} />);
    await user.type(input(), "/home/u/");
    await waitFor(() => expect(shown().length).toBeGreaterThan(0));
    await user.keyboard("{Enter}");
    expect(onSubmit.mock.calls).toEqual([["/home/u/"]]);
    expect(input().value).toBe("/home/u/");
  });

  // spec: UI-023
  test("Escape closes the list and keeps the value", async () => {
    const user = userEvent.setup();
    const source = sources();
    render(<Harness source={source} initial="/" withSuggest={false} />);
    await user.click(input());
    await waitFor(() => expect(shown().length).toBe(2));
    await user.keyboard("{Escape}");
    expect(shown()).toEqual([]);
    expect(input().value).toBe("/");
    expect(document.activeElement).toBe(input());
  });
});
