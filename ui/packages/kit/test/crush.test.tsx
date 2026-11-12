import { afterEach, describe, expect, mock, test } from "bun:test";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import {
  BorderBox,
  BoxRule,
  Button,
  Card,
  Checkbox,
  Code,
  CommandItem,
  ConfirmDialog,
  compactNumber,
  Diff,
  DiffStat,
  ExitCode,
  HelpDialog,
  HelpRow,
  HelpSection,
  InteractionCard,
  Log,
  Markdown,
  Menu,
  MenuItem,
  Meter,
  ModeLine,
  ModeNotice,
  ModeSegment,
  motionLevel,
  PathField,
  ResolvedLine,
  Select,
  StateBadge,
  StateMark,
  StreamCaret,
  Tab,
  TabList,
  Tabs,
  TextField,
  Thinking,
  ThreadRow,
  Toast,
  ToolCard,
  WorkspaceSection,
} from "../src/index.ts";

afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.motion;
});

describe("Button", () => {
  // spec: UI-149
  test("shows a leading icon and a trailing key chip outside its accessible name", () => {
    render(
      <Button variant="primary" icon={<svg data-testid="icon" />} kbd="enter">
        Send
      </Button>,
    );
    const button = screen.getByRole("button", { name: "Send" });
    expect(button.getAttribute("data-variant")).toBe("primary");
    expect(button.querySelector("[data-testid=icon]")).not.toBeNull();
    const chip = button.querySelector("kbd");
    expect(chip?.textContent).toBe("enter");
    expect(chip?.getAttribute("aria-hidden")).toBe("true");
  });

  // spec: UI-149, UI-133
  test("busy keeps the label, spins an icon and ignores presses", async () => {
    const user = userEvent.setup();
    const onPress = mock(() => undefined);
    render(
      <Button isPending onPress={onPress} data-testid="go">
        Remove
      </Button>,
    );
    const button = screen.getByTestId("go");
    expect(button.textContent).toBe("Remove");
    expect(button.hasAttribute("data-pending")).toBe(true);
    expect(button.querySelector(".omfp-button__spinner")).not.toBeNull();
    await user.click(button);
    expect(onPress).not.toHaveBeenCalled();
  });
});

describe("TextField", () => {
  // spec: UI-150
  test("a box with icon, key chip and mono text; invalid shows an error line with an alert icon", () => {
    render(
      <TextField
        label="Path"
        icon={<svg data-testid="icon" />}
        kbd="tab"
        mono
        errorMessage="No such folder"
        defaultValue="~/projects"
        data-testid="field"
      />,
    );
    const field = screen.getByTestId("field");
    const box = field.querySelector(".omfp-input-box");
    expect(box?.getAttribute("data-mono")).toBe("true");
    expect(box?.querySelector("[data-testid=icon]")).not.toBeNull();
    expect(box?.querySelector("kbd")?.textContent).toBe("tab");
    expect(field.hasAttribute("data-invalid")).toBe(true);
    const error = field.querySelector(".omfp-field__error");
    expect(error?.textContent).toBe("No such folder");
    expect(error?.querySelector("[data-icon=circle-alert]")).not.toBeNull();
    expect(screen.getByRole("textbox", { name: "Path" })).toBeDefined();
  });
});

describe("Select", () => {
  // spec: UI-152
  test("shows its prefix, value and chevron; the open list checks the selected option and shows metadata", async () => {
    const user = userEvent.setup();
    render(
      <Select
        label="Thinking"
        hideLabel
        prefix="thinking"
        items={[
          { id: "low", label: "low" },
          { id: "high", label: "high", meta: "slow" },
        ]}
        selectedKey="high"
        onSelectionChange={() => undefined}
        data-testid="select"
      />,
    );
    const select = screen.getByTestId("select");
    expect(select.querySelector(".omfp-select__prefix")?.textContent).toBe("thinking");
    expect(select.querySelector(".omfp-select__chevron")).not.toBeNull();
    await user.click(screen.getByRole("button"));
    const options = await screen.findAllByRole("option");
    expect(options.map((option) => option.querySelector(".omfp-option__check svg") !== null)).toEqual([false, true]);
    expect(options[1]?.querySelector(".omfp-option__meta")?.textContent).toBe("slow");
  });
});

describe("Checkbox", () => {
  // spec: UI-153
  test("checked and mixed states are exposed for the box's check and dash", () => {
    render(
      <>
        <Checkbox defaultSelected data-testid="checked">
          Checked
        </Checkbox>
        <Checkbox isIndeterminate data-testid="mixed">
          Mixed
        </Checkbox>
      </>,
    );
    expect(screen.getByTestId("checked").hasAttribute("data-selected")).toBe(true);
    expect(screen.getByTestId("checked").querySelector(".omfp-checkbox__check")).not.toBeNull();
    expect(screen.getByTestId("mixed").hasAttribute("data-indeterminate")).toBe(true);
    expect(screen.getByTestId("mixed").querySelector(".omfp-checkbox__dash")).not.toBeNull();
  });
});

describe("Tabs", () => {
  // spec: UI-154
  test("the selected tab is marked for its glyph and indicator", () => {
    render(
      <Tabs defaultSelectedKey="changes">
        <TabList aria-label="Panels">
          <Tab id="workspace">Workspace</Tab>
          <Tab id="changes">Changes</Tab>
        </TabList>
      </Tabs>,
    );
    expect(screen.getByRole("tab", { name: "Changes" }).hasAttribute("data-selected")).toBe(true);
    expect(screen.getByRole("tab", { name: "Workspace" }).hasAttribute("data-selected")).toBe(false);
  });
});

describe("Menu", () => {
  // spec: UI-155
  test("items show an icon, a label and a key chip; destructive items are marked", () => {
    render(
      <Menu aria-label="Workspace">
        <MenuItem id="rename" icon={<svg data-testid="icon" />} kbd="r">
          Rename
        </MenuItem>
        <MenuItem id="remove" danger kbd="d d">
          Remove workspace
        </MenuItem>
      </Menu>,
    );
    const rename = screen.getByRole("menuitem", { name: "Rename" });
    expect(rename.querySelector(".omfp-menu__label")?.textContent).toBe("Rename");
    expect(rename.querySelector("kbd")?.textContent).toBe("r");
    expect(screen.getByRole("menuitem", { name: "Remove workspace" }).getAttribute("data-danger")).toBe("true");
  });
});

describe("ConfirmDialog", () => {
  // spec: UI-156
  test("confirming: the confirm button is busy and the dialog stays; an error shows above the actions", async () => {
    const user = userEvent.setup();
    const onConfirm = mock(() => undefined);
    render(
      <ConfirmDialog
        isOpen
        onOpenChange={() => undefined}
        title="Remove?"
        message="Its folder is deleted."
        confirmLabel="Remove"
        tone="danger"
        cancelKbd="esc"
        confirmKbd="enter"
        busy
        error="The folder is busy"
        onConfirm={onConfirm}
      />,
    );
    const confirm = screen.getByTestId("confirm");
    expect(confirm.hasAttribute("data-pending")).toBe(true);
    expect(screen.getByTestId("cancel").querySelector("kbd")?.textContent).toBe("esc");
    expect(screen.getByRole("alert").textContent).toBe("The folder is busy");
    await user.click(confirm);
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByTestId("confirm-dialog")).toBeDefined();
  });

  // spec: UI-156, UI-173
  test("renders in place without the modal overlay", () => {
    render(
      <ConfirmDialog
        inline
        isOpen
        onOpenChange={() => undefined}
        title="Remove?"
        message="Gone."
        confirmLabel="Remove"
        onConfirm={() => undefined}
      />,
    );
    expect(document.querySelector(".omfp-modal-overlay")).toBeNull();
    expect(screen.getByRole("alertdialog", { name: "Remove?" })).toBeDefined();
  });
});

describe("marks", () => {
  // spec: UI-122, UI-137, UI-157
  test("state marks and badges carry their state; decorative unless labelled", () => {
    render(
      <>
        <StateMark state="running" data-testid="mark" />
        <StateMark state="failed" label="Failed" />
        <StateBadge state="awaiting" fresh data-testid="badge" />
      </>,
    );
    expect(screen.getByTestId("mark").getAttribute("data-state")).toBe("running");
    expect(screen.getByTestId("mark").getAttribute("aria-hidden")).toBe("true");
    expect(screen.getByRole("img", { name: "Failed" }).getAttribute("data-state")).toBe("failed");
    const badge = screen.getByTestId("badge");
    expect(badge.textContent).toBe("awaiting");
    expect(badge.getAttribute("data-fresh")).toBe("true");
  });

  // spec: UI-137, UI-157
  test("exit codes: 0 is ok, anything else is not", () => {
    render(
      <>
        <ExitCode code={0} data-testid="ok" />
        <ExitCode code={2} data-testid="err" />
      </>,
    );
    expect(screen.getByTestId("ok").textContent).toBe("exit 0");
    expect(screen.getByTestId("ok").getAttribute("data-ok")).toBe("true");
    expect(screen.getByTestId("err").getAttribute("data-ok")).toBe("false");
  });

  // spec: UI-158
  test("token counts are compact", () => {
    expect([0, 999, 1000, 48_200, 171_000, 200_000, 1_500_000].map(compactNumber)).toEqual([
      "0",
      "999",
      "1k",
      "48.2k",
      "171k",
      "200k",
      "1.5M",
    ]);
  });

  // spec: UI-158
  test("the meter shows used/total and turns high above its threshold", () => {
    render(
      <>
        <Meter value={48_200} max={200_000} label="Context" data-testid="low" />
        <Meter value={171_000} max={200_000} label="Context" data-testid="high" />
      </>,
    );
    expect(screen.getByTestId("low").textContent).toBe("48.2k/200k");
    expect(screen.getByTestId("low").hasAttribute("data-high")).toBe(false);
    expect(screen.getByTestId("high").getAttribute("data-high")).toBe("true");
    expect(screen.getAllByRole("meter")).toHaveLength(2);
  });

  // spec: UI-165, UI-145
  test("diff stats show their final numbers at once unless motion is full", () => {
    document.documentElement.dataset.motion = "reduced";
    render(<DiffStat added={42} removed={7} data-testid="stat" />);
    expect(screen.getByTestId("stat").textContent).toBe("+42−7");
  });

  // spec: UI-165, UI-146
  test("at motion full diff stats count up from 0", async () => {
    document.documentElement.dataset.motion = "full";
    render(<DiffStat added={42} removed={7} data-testid="stat" />);
    expect(screen.getByTestId("stat").textContent).toBe("+0−0");
    await waitFor(() => expect(screen.getByTestId("stat").textContent).toBe("+42−7"), { timeout: 2000 });
  });

  // spec: UI-145
  test("the effective motion level is data-motion, else the system preference", () => {
    document.documentElement.dataset.motion = "none";
    expect(motionLevel()).toBe("none");
    delete document.documentElement.dataset.motion;
    expect(motionLevel()).toBe("full");
  });
});

describe("Log", () => {
  // spec: UI-159
  test("structured lines have a time and a level column; an empty log shows one muted line", () => {
    const { rerender } = render(<Log lines={[]} data-testid="log" />);
    expect(screen.getByTestId("log").textContent).toBe("no output");
    rerender(<Log lines={[{ time: "14:02:11", level: "warn", text: "slow test" }, "plain line"]} data-testid="log" />);
    const log = screen.getByTestId("log");
    expect(log.querySelector(".omfp-log__level")?.getAttribute("data-level")).toBe("warn");
    expect([...log.querySelectorAll(".omfp-log__line")].map((line) => line.textContent)).toEqual([
      "14:02:11 warn slow test",
      "plain line",
    ]);
  });
});

describe("Diff", () => {
  // spec: UI-165
  test("an empty diff says so, a binary file too", () => {
    const { rerender } = render(<Diff patch="" data-testid="diff" />);
    expect(screen.getByTestId("diff").textContent).toBe("no changes");
    rerender(<Diff patch="" binary data-testid="diff" />);
    expect(screen.getByTestId("diff").textContent).toBe("binary file");
  });
});

describe("Toast", () => {
  // spec: UI-160
  test("info is a status that dismisses itself; errors are alerts that stay", async () => {
    const dismissed = mock(() => undefined);
    const stays = mock(() => undefined);
    render(
      <>
        <Toast onDismiss={dismissed} duration={20}>
          Saved
        </Toast>
        <Toast tone="error" onDismiss={stays} duration={1} action={<Button>Retry</Button>}>
          Daemon unreachable
        </Toast>
      </>,
    );
    expect(screen.getByRole("status").textContent).toContain("Saved");
    expect(screen.getByRole("alert").textContent).toContain("Daemon unreachable");
    // The error's own deadline (1 ms) passed before the info's (20 ms): it would have fired by now.
    await waitFor(() => expect(dismissed).toHaveBeenCalledTimes(1));
    expect(stays).not.toHaveBeenCalled();
  });
});

function ToggledTool({ body }: { body: boolean }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <ToolCard
      status="done"
      name="bash"
      arg="ls -la"
      meta="1.2s"
      expanded={expanded}
      onExpandedChange={setExpanded}
      data-testid="tool"
    >
      {body ? <pre data-testid="output">total 0</pre> : undefined}
    </ToolCard>
  );
}

describe("ToolCard", () => {
  // spec: UI-163
  test("the head toggles the body", async () => {
    const user = userEvent.setup();
    render(<ToggledTool body />);
    const head = screen.getByRole("button", { expanded: false });
    expect(screen.queryByTestId("output")).toBeNull();
    await user.click(head);
    expect(head.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByTestId("output")).toBeDefined();
    expect(screen.getByTestId("tool").getAttribute("data-status")).toBe("done");
  });

  // spec: UI-163
  test("a head without a body is not clickable", () => {
    render(<ToggledTool body={false} />);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getByTestId("tool-head").textContent).toBe("bashls -la1.2s");
    expect(screen.getByTestId("tool").classList.contains("omfp-bbox")).toBe(false);
  });

  // spec: UI-163, UI-216
  test("a running tool card's mark uses omp's status spinner; a thread mark keeps the activity spinner", () => {
    render(
      <>
        <ToolCard status="running" name="bash" data-testid="tool" />
        <StateMark state="running" data-testid="mark" />
      </>,
    );
    const toolMark = screen.getByTestId("tool").querySelector(".omfp-state-mark");
    expect(toolMark?.getAttribute("data-spinner")).toBe("status");
    expect(screen.getByTestId("mark").hasAttribute("data-spinner")).toBe(false);
  });

  // spec: UI-163, UI-187
  test("while the body shows, the whole head sits on the top border; collapsed, the card is one row again", async () => {
    const user = userEvent.setup();
    render(<ToggledTool body />);
    const tool = screen.getByTestId("tool");
    const head = screen.getByRole("button", { expanded: false });
    expect(tool.classList.contains("omfp-bbox")).toBe(false);
    expect(head.querySelector(".omfp-bbox__line")).toBeNull();
    await user.click(head);
    expect(tool.classList.contains("omfp-bbox")).toBe(true);
    expect(head.classList.contains("omfp-bbox__head")).toBe(true);
    expect(head.querySelector(".omfp-bbox__label")?.textContent).toBe("bashls -la1.2s");
    expect(head.querySelector(".omfp-bbox__line")?.getAttribute("aria-hidden")).toBe("true");
    expect(tool.querySelector(".omfp-tool__body")?.classList.contains("omfp-bbox__body")).toBe(true);
    expect(screen.getByTestId("tool-head")).toBe(head);
    await user.click(head);
    expect(tool.classList.contains("omfp-bbox")).toBe(false);
  });
});

describe("BorderBox", () => {
  // spec: UI-187
  test("the title is a heading on the top border, the end part after the line, the body under it", () => {
    render(
      <BorderBox title="Repositories" level={3} titleId="repos" end="0:42" aria-labelledby="repos" data-testid="box">
        <span data-testid="content">rows</span>
      </BorderBox>,
    );
    const box = screen.getByRole("region", { name: "Repositories" });
    expect(box).toBe(screen.getByTestId("box"));
    const head = box.querySelector(".omfp-bbox__head");
    expect([...(head?.children ?? [])].map((part) => part.className)).toEqual([
      "omfp-bbox__label",
      "omfp-bbox__line",
      "omfp-bbox__end",
    ]);
    expect(screen.getByRole("heading", { level: 3 }).id).toBe("repos");
    expect(head?.querySelector(".omfp-bbox__end")?.textContent).toBe("0:42");
    expect(screen.getByTestId("content").parentElement?.className).toBe("omfp-bbox__body");
  });

  // spec: UI-187
  test("a labelled rule is a heading that opens the next section", () => {
    render(<BoxRule label="Output" data-testid="rule" />);
    const rule = screen.getByRole("heading", { level: 4, name: "Output" });
    expect(rule).toBe(screen.getByTestId("rule"));
    expect(rule.className).toBe("omfp-bbox__rule");
  });
});

describe("HelpDialog", () => {
  // spec: UI-198, UI-187, UI-092
  test("a modal border box titled help, its sections on labelled rules; Escape asks to close", async () => {
    const onClose = mock(() => {});
    render(
      <HelpDialog label="Keys and commands" filter={<input aria-label="Filter" />} end="esc" onClose={onClose}>
        <HelpSection title="normal" note="vim keys">
          <HelpRow keys={["spc a", "] a"]} description="Next thread needing attention" />
          <HelpRow keys={[":theme"]} args="system | light | dark" description="Colour scheme" />
        </HelpSection>
      </HelpDialog>,
    );
    const dialog = screen.getByRole("dialog", { name: "Keys and commands" });
    expect(dialog.querySelector(".omfp-bbox__label")?.textContent).toBe("help");
    expect(dialog.querySelector(".omfp-bbox__end")?.textContent).toBe("esc");
    expect(screen.getByRole("heading", { level: 3, name: "normal" }).className).toBe("omfp-bbox__rule");
    expect([...dialog.querySelectorAll(".omfp-help__row")].map((row) => row.textContent)).toEqual([
      "spc a] aNext thread needing attention",
      ":themesystem | light | darkColour scheme",
    ]);
    expect([...dialog.querySelectorAll(".omfp-help__row kbd")].map((chip) => chip.className)).toEqual([
      "omfp-kbd",
      "omfp-kbd",
      "omfp-kbd",
    ]);
    // The scrolling list is a Tab stop, so the keyboard can scroll it.
    expect(screen.getByRole("region", { name: "Keys and commands" }).getAttribute("tabindex")).toBe("0");
    await userEvent.setup().keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});

describe("approval and ask cards", () => {
  // spec: UI-164
  test("the countdown turns low; a resolved line keeps the question and the answer", () => {
    render(
      <>
        <InteractionCard title="Run a command?" meta="1/2" countdown="0:04" low data-testid="card" />
        <ResolvedLine outcome="denied" question="Run a command?" answer="Deny" data-testid="resolved" />
      </>,
    );
    expect(screen.getByTestId("card").querySelector("[data-low]")?.textContent).toBe("0:04");
    const resolved = screen.getByTestId("resolved");
    expect(resolved.getAttribute("data-outcome")).toBe("denied");
    expect(resolved.querySelector("[data-icon=x]")).not.toBeNull();
    expect(resolved.textContent).toBe("Run a command?Deny");
  });

  // spec: UI-164, UI-187
  test("the `◆` title sits on the top border, the step and countdown at its right end", () => {
    render(
      <InteractionCard title="Run a command?" meta="1/2" countdown="0:42" data-testid="card">
        <button type="button">Approve</button>
      </InteractionCard>,
    );
    const card = screen.getByTestId("card");
    expect(card.classList.contains("omfp-bbox")).toBe(true);
    expect(card.querySelector(".omfp-bbox__label")?.textContent).toBe("◆ Run a command?");
    expect(card.querySelector(".omfp-bbox__end")?.textContent).toBe("1/20:42");
    expect(screen.getByRole("button", { name: "Approve" }).parentElement?.classList.contains("omfp-bbox__body")).toBe(
      true,
    );
  });
});

/** A workspace section that toggles itself. */
function ToggledSection() {
  const [expanded, setExpanded] = useState(true);
  return (
    <WorkspaceSection name="omfp" path="~/ws/omfp" count={3} current expanded={expanded} onExpandedChange={setExpanded}>
      <span data-testid="rows">rows</span>
    </WorkspaceSection>
  );
}

/** A thinking block that toggles itself. */
function ToggledThinking() {
  const [expanded, setExpanded] = useState(false);
  return (
    <Thinking label="Thinking" meta="4s" expanded={expanded} onExpandedChange={setExpanded}>
      Considering the options.
    </Thinking>
  );
}

describe("shell", () => {
  // spec: UI-167
  test("the mode line shows the mode pill and its segments", () => {
    render(
      <ModeLine mode="insert" data-testid="line">
        <ModeSegment kind="title">Fix it</ModeSegment>
        <ModeSegment kind="error">No such command</ModeSegment>
      </ModeLine>,
    );
    const line = screen.getByTestId("line");
    expect(line.querySelector(".omfp-mode-pill")?.textContent).toBe("INSERT");
    expect([...line.querySelectorAll("[data-kind]")].map((segment) => segment.getAttribute("data-kind"))).toEqual([
      "title",
      "error",
    ]);
  });

  // spec: UI-193
  test("a mode-line notice sits after the segments with its tone, count and key chip, and opens on press", async () => {
    const user = userEvent.setup();
    const onPress = mock(() => undefined);
    render(
      <ModeLine
        mode="normal"
        data-testid="line"
        notice={<ModeNotice tone="error" title="flaky" body="failed" more={2} kbd="g n" onPress={onPress} />}
      >
        <ModeSegment kind="title">Fix it</ModeSegment>
      </ModeLine>,
    );
    const line = screen.getByTestId("line");
    expect([...line.children].map((child) => child.className)).toEqual([
      "omfp-mode-pill",
      "omfp-modeline__segments",
      "omfp-modeline__notice",
    ]);
    const notice = screen.getByRole("button", { name: "flaky failed +2" });
    expect(notice.getAttribute("data-tone")).toBe("error");
    expect(notice.querySelector("kbd")?.textContent).toBe("g n");
    await user.click(notice);
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  // spec: UI-168
  test("a completion bolds the matched part and is selected for the ▸", () => {
    render(<CommandItem command="rescan" matched={2} description="Rescan" selected data-testid="item" />);
    const item = screen.getByTestId("item");
    expect(item.getAttribute("aria-selected")).toBe("true");
    expect(item.querySelector("b")?.textContent).toBe("re");
  });

  // spec: UI-169
  test("a thread row carries its state and selection and offers delete", async () => {
    const user = userEvent.setup();
    const onDelete = mock(() => undefined);
    render(
      <ThreadRow state="awaiting" title="Fix it" meta="2m" selected fresh onDelete={onDelete} data-testid="row" />,
    );
    const row = screen.getByTestId("row");
    expect(row.getAttribute("data-state")).toBe("awaiting");
    expect(row.getAttribute("data-selected")).toBe("true");
    expect(row.getAttribute("data-fresh")).toBe("true");
    await user.click(screen.getByRole("button", { name: "Delete thread" }));
    expect(onDelete).toHaveBeenCalledTimes(1);
  });

  // spec: UI-170
  test("a workspace head toggles its rows with ▾ and ▸", async () => {
    const user = userEvent.setup();
    render(<ToggledSection />);
    const head = screen.getByTestId("workspace-head");
    expect(head.textContent).toBe("▾omfp~/ws/omfp3");
    await user.click(head);
    expect(head.getAttribute("aria-expanded")).toBe("false");
    expect(head.textContent?.startsWith("▸")).toBe(true);
    expect(screen.queryByTestId("rows")).toBeNull();
  });

  // spec: UI-171
  test("a thinking block expands to its text", async () => {
    const user = userEvent.setup();
    render(<ToggledThinking />);
    expect(screen.queryByText("Considering the options.")).toBeNull();
    await user.click(screen.getByTestId("thinking-head"));
    expect(screen.getByText("Considering the options.")).toBeDefined();
  });
});

/** A path field completing in a fixed folder. */
function CompletingPathField() {
  const [value, setValue] = useState("");
  return (
    <PathField
      label="Folder"
      value={value}
      onChange={setValue}
      complete={async () => [
        { name: "pmo", path: "/home/u/projects/pmo", isGitRoot: true },
        { name: "papers", path: "/home/u/projects/papers", isGitRoot: false },
      ]}
      data-testid="path"
    />
  );
}

describe("path field", () => {
  // spec: UI-151
  test("a mono field with a folder icon and a tab chip; completions mute the typed folder and bold the match", async () => {
    const user = userEvent.setup();
    render(<CompletingPathField />);
    const box = document.querySelector(".omfp-input-box");
    expect(box?.getAttribute("data-mono")).toBe("true");
    expect(box?.querySelector("[data-icon=folder-open]")).not.toBeNull();
    expect(box?.querySelector("kbd")?.textContent).toBe("tab");
    await user.type(screen.getByTestId("path"), "~/projects/p");
    const options = await screen.findAllByTestId("path-option");
    expect(options.map((option) => option.querySelector(".omfp-path-field__dir")?.textContent)).toEqual([
      "~/projects/",
      "~/projects/",
    ]);
    expect(options.map((option) => option.querySelector("b")?.textContent)).toEqual(["p", "p"]);
    expect(options.map((option) => option.querySelector(".omfp-path-field__meta")?.textContent)).toEqual([
      "git",
      "folder",
    ]);
  });
});

describe("cards and code", () => {
  // spec: UI-162
  test("a card has a head row and a body", () => {
    render(
      <Card title="Card" actions={<span>3</span>} data-testid="card">
        body
      </Card>,
    );
    const card = screen.getByTestId("card");
    expect(card.querySelector(".omfp-card__header")?.textContent).toBe("Card3");
    expect(card.querySelector(".omfp-card__body")?.textContent).toBe("body");
  });

  // spec: UI-166
  test("a code block names its language for the top-right corner, fenced Markdown too", () => {
    render(
      <>
        <Code block language="ts" code="const a = 1;" data-testid="block" />
        <Markdown text={"```rust\nfn main() {}\n```"} data-testid="markdown" />
      </>,
    );
    expect(screen.getByTestId("block").getAttribute("data-lang")).toBe("ts");
    expect(screen.getByTestId("block").textContent).toBe("const a = 1;");
    expect(screen.getByTestId("markdown").querySelector("pre")?.getAttribute("data-lang")).toBe("rust");
  });

  // spec: UI-147
  test("the stream caret is decorative", () => {
    render(<StreamCaret data-testid="caret" />);
    expect(screen.getByTestId("caret").getAttribute("aria-hidden")).toBe("true");
  });
});
