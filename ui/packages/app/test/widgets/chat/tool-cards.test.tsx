/**
 * L4: the specialized tool cards (UI-050..052) rendered from fixtures recorded from the pinned omp (UI-051), each in
 * its streaming, success and error states; code in them is highlighted (UI-090).
 */

import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { type OmfpHost, OPEN_FILES, SELECTED_THREAD } from "../../../src/sdk/index.ts";
import type { ToolCard } from "../../../src/vm/tool.ts";
import { MessageRow, ToolCardView } from "../../../src/widgets/chat/message.tsx";
import { card, streaming } from "../../support/cards.ts";
import { repo, thread, workspace } from "../../support/fixtures.ts";
import { renderWithHost, testHost } from "../../support/host.tsx";

afterEach(cleanup);

/** The host of the last {@link show}. */
let shownHost: OmfpHost | undefined;

/** Renders `toolCard` in workspace `w1` (folder `/work/w1`, whose repository `src` holds the fixtures' files). */
function show(toolCard: ToolCard): HTMLElement {
  const test = testHost();
  shownHost = test.host;
  test.client.connected();
  test.client.index({
    workspaces: [workspace("w1", [{ ...repo("w1", "src", "main"), kind: "clone" }])],
    threads: [thread("t1", "cards", "w1")],
  });
  test.host.window.set(SELECTED_THREAD, "t1");
  renderWithHost(<ToolCardView cardKey={toolCard.toolCallId} card={toolCard} />, test);
  test.flush();
  return screen.getByTestId("tool-card");
}

/** Renders the card and opens it. */
async function open(toolCard: ToolCard): Promise<HTMLElement> {
  const element = show(toolCard);
  if (element.querySelector("[aria-expanded=false]") !== null) {
    await userEvent.click(within(element).getByTestId("toggle"));
  }
  return element;
}

const byTestId = (element: HTMLElement, id: string) => within(element).getByTestId(id);
const texts = (element: HTMLElement, id: string) =>
  within(element)
    .queryAllByTestId(id)
    .map((node) => node.textContent);

describe("bash card", () => {
  // spec: UI-050, UI-051
  test("streaming: the command in the header, the output so far, no exit code yet", async () => {
    const element = await open(streaming("bash_tool_call", "call_bash_1"));
    expect([element.dataset.view, element.dataset.status]).toEqual(["bash", "running"]);
    expect(byTestId(element, "tool-subject").textContent).toBe("echo tool-output-123");
    expect(byTestId(element, "tool-output").textContent).toBe("tool-output-123\n");
    expect(within(element).queryByTestId("tool-exit")).toBeNull();
  });

  // spec: UI-050, UI-051, UI-137, UI-163
  test("success: collapsed, then the output and exit 0", async () => {
    const done = card("bash_tool_call", "call_bash_1");
    expect(within(show(done)).queryByTestId("tool-output")).toBeNull();
    cleanup();
    const element = await open(done);
    expect(byTestId(element, "tool-exit").textContent).toBe("exit 0");
    expect(byTestId(element, "tool-exit").dataset.ok).toBe("true");
  });

  // spec: UI-050, UI-051, UI-052, UI-137, UI-163
  test("error: expanded and distinct, with the exit code", () => {
    const element = show(card("bash_tool_error", "call_bash_1"));
    expect(element.dataset.status).toBe("failed");
    expect(element.querySelector(".omfp-tool")?.getAttribute("data-status")).toBe("failed");
    expect(byTestId(element, "tool-output").textContent).toStartWith("oops\n");
    expect(byTestId(element, "tool-exit").textContent).toBe("exit 3");
    expect(byTestId(element, "tool-exit").dataset.ok).toBe("false");
  });

  // spec: UI-050
  test("ANSI colors become theme classes", async () => {
    const done = card("bash_tool_call", "call_bash_1");
    const colored: ToolCard = {
      ...done,
      view: { kind: "bash", command: "ls", cwd: "/tmp", output: "\u001b[1;34mdir\u001b[0m file", exitCode: 0 },
    };
    const element = await open(colored);
    const output = byTestId(element, "tool-output");
    expect(output.textContent).toBe("dir file");
    expect(output.querySelector("span")?.className).toBe("omfp-ansi-fg-blue omfp-ansi-bold");
    expect(element.textContent).toContain("in /tmp");
  });
});

describe("read card", () => {
  // spec: UI-050, UI-051
  test("streaming: the path relative to its repository, with the repository chip", () => {
    const element = show(streaming("read_tool_calls", "call_read_1"));
    expect(element.dataset.status).toBe("running");
    expect(byTestId(element, "tool-repo").textContent).toBe("src");
    expect(byTestId(element, "tool-path").textContent).toBe("srclib.rs");
  });

  // spec: UI-050, UI-051, UI-090
  test("success: the result is collapsed, then the highlighted lines and their range", async () => {
    const done = card("read_tool_calls", "call_read_1");
    expect(within(show(done)).queryByTestId("tool-content")).toBeNull();
    cleanup();
    const element = await open(done);
    expect(byTestId(element, "tool-range").textContent).toBe("1–9 of 9");
    const content = byTestId(element, "tool-content");
    expect(content.textContent).toStartWith("//! Tool card fixture crate.");
    await waitFor(() => expect(content.querySelector(".hljs-keyword")?.textContent).toBe("pub"));
  });

  // spec: UI-050, UI-051, UI-052
  test("error: omp's message, expanded", () => {
    const element = show(card("read_tool_calls", "call_read_2"));
    expect(byTestId(element, "tool-error").textContent).toBe("Path 'src/missing.rs' not found");
    expect(within(element).queryByTestId("tool-content")).toBeNull();
  });
});

describe("fetch card", () => {
  // spec: UI-050, UI-051
  test("streaming and success: the URL, then the page", async () => {
    expect(byTestId(show(streaming("web_tool_calls", "call_read_1")), "tool-subject").textContent).toBe(
      "http://127.0.0.1:29417/page",
    );
    cleanup();
    const element = await open(card("web_tool_calls", "call_read_1"));
    expect(byTestId(element, "tool-link").getAttribute("href")).toBe("http://127.0.0.1:29417/page");
    expect(byTestId(element, "tool-content").textContent).toContain("# Tool cards");
  });

  // spec: UI-050, UI-051, UI-052
  test("error: a 404 is a failure with omp's note", () => {
    const element = show(card("web_tool_calls", "call_read_2"));
    expect(element.dataset.status).toBe("failed");
    expect(byTestId(element, "tool-error").textContent).toContain("Method: failed");
  });
});

describe("edit card", () => {
  // spec: UI-050, UI-051
  test("streaming: the file and the edit as written", async () => {
    const element = await open(streaming("edit_tool_calls", "call_edit_1"));
    expect(byTestId(element, "tool-path").textContent).toBe("srclib.rs");
    expect(byTestId(element, "tool-input").textContent).toContain("left.saturating_add(right)");
  });

  // spec: UI-050, UI-051, UI-090
  test("success: a highlighted diff view", async () => {
    const element = await open(card("edit_tool_calls", "call_edit_1"));
    const diff = byTestId(element, "tool-diff");
    expect([...diff.querySelectorAll("[data-kind=del]")].map((row) => row.textContent)).toEqual(["4−    left + right"]);
    expect([...diff.querySelectorAll("[data-kind=add]")].map((row) => row.textContent)).toEqual([
      "4+    left.saturating_add(right)",
    ]);
    await waitFor(() => expect(diff.querySelector(".hljs-keyword")?.textContent).toBe("pub"));
  });

  // spec: UI-050, UI-051, UI-052
  test("error: omp's reason and the refused edit", () => {
    const element = show(card("edit_tool_calls", "call_edit_2"));
    expect(byTestId(element, "tool-error").textContent).toContain("is not from this session");
    expect(byTestId(element, "tool-input").textContent).toContain("PUT 8.=8:");
  });
});

describe("write card", () => {
  // spec: UI-050, UI-051, UI-090
  test("streaming and success: the file and its highlighted content", async () => {
    expect(byTestId(show(streaming("write_tool_calls", "call_write_1")), "tool-path").textContent).toBe("notes.md");
    cleanup();
    const element = await open(card("write_tool_calls", "call_write_1"));
    const content = byTestId(element, "tool-content");
    expect(content.textContent).toBe("# Notes\n\n- add saturates\n- greet says hello\n");
    await waitFor(() => expect(content.querySelector(".hljs-section")?.textContent).toBe("# Notes"));
  });

  // spec: UI-053
  test("long content: the first 6 lines, then the whole content on demand", async () => {
    const recorded = card("write_tool_calls", "call_write_1");
    const content = `${Array.from({ length: 9 }, (_, at) => `line ${at + 1}`).join("\n")}\n`;
    const element = await open({ ...recorded, view: { ...recorded.view, content } as ToolCard["view"] });
    expect(byTestId(element, "tool-content").textContent).toBe("line 1\nline 2\nline 3\nline 4\nline 5\nline 6");
    const more = byTestId(element, "tool-more");
    expect([more.textContent, more.getAttribute("aria-expanded")]).toEqual(["… 3 more lines", "false"]);
    await userEvent.click(more);
    expect(byTestId(element, "tool-content").textContent).toBe(content);
    await userEvent.click(byTestId(element, "tool-more"));
    expect(byTestId(element, "tool-content").textContent).toEndWith("line 6");
  });

  // spec: UI-053
  test("short content: no expander", async () => {
    const element = await open(card("write_tool_calls", "call_write_1"));
    expect(within(element).queryByTestId("tool-more")).toBeNull();
  });

  // spec: UI-050, UI-051, UI-052
  test("error: writing over a directory", () => {
    expect(byTestId(show(card("write_tool_calls", "call_write_2")), "tool-error").textContent).toContain("EISDIR");
  });
});

describe("search cards", () => {
  // spec: UI-050, UI-051
  test("streaming: the query; success: the files with their match counts", async () => {
    expect(byTestId(show(streaming("search_tool_calls", "call_grep_1")), "tool-subject").textContent).toBe("pub fn");
    cleanup();
    const grep = await open(card("search_tool_calls", "call_grep_1"));
    expect(byTestId(grep, "tool-total").textContent).toBe("2 matches in 1 file");
    expect(texts(grep, "tool-file")).toEqual(["srclib.rs2"]);
    cleanup();
    const glob = await open(card("search_tool_calls", "call_glob_1"));
    expect([byTestId(glob, "tool-subject").textContent, byTestId(glob, "tool-total").textContent]).toEqual([
      "src/**/*.rs",
      "1 file",
    ]);
  });

  // spec: UI-050, UI-051, UI-052
  test("error: a missing folder", () => {
    expect(byTestId(show(card("search_tool_calls", "call_glob_2")), "tool-error").textContent).toBe(
      "Path not found: missing",
    );
  });
});

describe("todo card", () => {
  // spec: UI-050, UI-051
  test("streaming: no list yet; success: the checklist", async () => {
    expect(texts(await open(streaming("todo_tool_calls", "call_todo_1")), "todo-task")).toEqual([]);
    cleanup();
    const element = await open(card("todo_tool_calls", "call_todo_3"));
    expect(
      within(element)
        .getAllByTestId("todo-task")
        .map((task) => [task.dataset.status, task.textContent]),
    ).toEqual([
      ["completed", "●Read the codedone"],
      ["in_progress", "●Fix the bugin progress"],
    ]);
  });

  // spec: UI-050, UI-051, UI-052
  test("error: an unknown task, with the list kept", () => {
    const element = show(card("todo_tool_calls", "call_todo_2"));
    expect(byTestId(element, "tool-error").textContent).toStartWith('Errors: Task "Write the docs" not found');
    expect(texts(element, "todo-task").length).toBe(2);
  });
});

describe("web_search card", () => {
  // spec: UI-050, UI-051
  test("streaming: the query; success: the links", async () => {
    expect(byTestId(show(streaming("web_tool_calls", "call_web_search_1")), "tool-subject").textContent).toBe(
      "omfp tool cards",
    );
    cleanup();
    const element = await open(card("web_tool_calls", "call_web_search_1"));
    const links = within(element).getAllByTestId("tool-link");
    expect(links.map((link) => link.querySelector("a")?.getAttribute("href"))).toEqual([
      "http://127.0.0.1:29417/page",
      "http://127.0.0.1/tools",
    ]);
    expect(links[0]?.textContent).toContain("Tool cards");
  });

  // spec: UI-050, UI-051, UI-052
  test("error: the engine's failure, expanded", () => {
    const element = show(card("web_tool_calls", "call_web_search_2"));
    expect(element.querySelector(".omfp-tool")?.getAttribute("data-status")).toBe("failed");
    expect(byTestId(element, "tool-error").textContent).toContain("SearXNG API error (500)");
  });
});

describe("ask card", () => {
  // spec: UI-050, UI-051
  test("streaming: the questions; success: with the given answers", async () => {
    expect(texts(await open(streaming("ask_answered", "call_ask_1")), "ask-answer")).toEqual([]);
    cleanup();
    const element = await open(card("ask_answered", "call_ask_1"));
    expect(texts(element, "ask-question")).toEqual(["Which database?SQLite", "Which features?no answer"]);
  });

  // spec: UI-050, UI-051, UI-052
  test("error: cancelled by the user", () => {
    const element = show(card("ask_cancelled", "call_ask_1"));
    expect(byTestId(element, "tool-error").textContent).toBe("Ask tool was cancelled by the user");
    expect(texts(element, "ask-question")).toEqual(["Which database?"]);
  });
});

describe("task card", () => {
  // spec: UI-050, UI-051
  test("streaming: each subagent's name, task and live progress; success: its output", async () => {
    const running = await open(streaming("task_tool_call", "call_task_1"));
    expect(byTestId(running, "tool-subject").textContent).toBe("Counter");
    expect(byTestId(running, "subagent-activity").textContent).toBe("Reading the library");
    cleanup();
    const element = await open(card("task_tool_call", "call_task_1"));
    expect(byTestId(element, "subagent").dataset.status).toBe("completed");
    expect(byTestId(element, "subagent-output").textContent).toBe("src/lib.rs defines 2 functions: add and greet.");
  });

  // spec: UI-050, UI-051, UI-052
  test("error: an unknown agent", () => {
    const element = show(card("task_tool_call", "call_task_2"));
    expect(byTestId(element, "tool-error").textContent).toContain('Unknown agent "no-such-agent"');
    expect(byTestId(element, "subagent").dataset.status).toBe("pending");
  });
});

describe("opening files from cards", () => {
  /** The files the last shown card opened. */
  const opened = () => shownHost?.window.get(OPEN_FILES).w1?.tabs.map(({ path, line, diff }) => ({ path, line, diff }));

  // spec: UI-116
  test("a read's path opens the file at its first line, without toggling the card; the card carries it for g f", async () => {
    const element = show(card("read_tool_calls", "call_read_1"));
    expect([element.dataset.omfpPath, element.dataset.omfpLine]).toEqual(["src/lib.rs", "1"]);
    await userEvent.click(byTestId(element, "tool-path-open"));
    expect(opened()).toEqual([{ path: "src/lib.rs", line: 1, diff: undefined }]);
    expect(within(element).getByTestId("toggle").getAttribute("aria-expanded")).toBe("false");
  });

  // spec: UI-116
  test("an edit's and a write's path open the file against HEAD", async () => {
    await userEvent.click(byTestId(show(card("edit_tool_calls", "call_edit_1")), "tool-path-open"));
    expect(opened()).toEqual([{ path: "src/lib.rs", line: undefined, diff: true }]);
    cleanup();
    const write = show(streaming("write_tool_calls", "call_write_1"));
    expect(write.dataset.omfpDiff).toBe("");
    await userEvent.click(byTestId(write, "tool-path-open"));
    expect(opened()).toEqual([{ path: "notes.md", line: undefined, diff: true }]);
  });

  // spec: UI-116
  test("paths in the assistant's Markdown open the file; the user's text stays plain", async () => {
    const test = testHost();
    test.client.connected();
    test.client.index({ workspaces: [workspace("w1")], threads: [thread("t1", "cards", "w1")] });
    test.host.window.set(SELECTED_THREAD, "t1");
    shownHost = test.host;
    renderWithHost(
      <>
        <MessageRow
          cardKey="m1"
          streaming={false}
          view={{
            role: "assistant",
            label: "Assistant",
            blocks: [{ type: "text", text: "Changed `src/lib.rs:4`." }],
            error: undefined,
            steering: false,
          }}
        />
        <MessageRow
          cardKey="m2"
          streaming={false}
          view={{
            role: "user",
            label: "You",
            blocks: [{ type: "text", text: "`src/lib.rs`" }],
            error: undefined,
            steering: false,
          }}
        />
      </>,
      test,
    );
    test.flush();
    const links = screen.getAllByTestId("path-link");
    expect(links.map((link) => link.textContent)).toEqual(["src/lib.rs:4"]);
    await userEvent.click(links[0] as HTMLElement);
    expect(opened()).toEqual([{ path: "src/lib.rs", line: 4, diff: undefined }]);
  });

  // spec: UI-116
  test("each file of a grep result opens", async () => {
    const grep = await open(card("search_tool_calls", "call_grep_1"));
    await userEvent.click(within(byTestId(grep, "tool-file")).getByTestId("path-link"));
    expect(opened()).toEqual([{ path: "src/lib.rs", line: undefined, diff: undefined }]);
  });
});
