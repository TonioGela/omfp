import { afterEach, describe, expect, mock, test } from "bun:test";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useEffect, useState } from "react";
import {
  Button,
  Card,
  Checkbox,
  Chip,
  Disclosure,
  type Key,
  ListBox,
  ListBoxItem,
  LiveRegion,
  Log,
  Menu,
  MenuItem,
  MenuTrigger,
  Popover,
  Progress,
  Radio,
  RadioGroup,
  SearchField,
  Select,
  Tab,
  TabList,
  TabPanel,
  Tabs,
  TextArea,
  TextField,
  Tooltip,
  TooltipTrigger,
} from "../src/index.ts";

afterEach(cleanup);

function focused(): string | null {
  return document.activeElement?.getAttribute("data-testid") ?? document.activeElement?.tagName ?? null;
}

describe("Button", () => {
  // spec: UI-092
  test("is reached with Tab and pressed with Enter and Space", async () => {
    const user = userEvent.setup();
    const onPress = mock(() => undefined);
    render(
      <Button data-testid="go" variant="primary" onPress={onPress}>
        Go
      </Button>,
    );
    await user.tab();
    expect(focused()).toBe("go");
    await user.keyboard("{Enter}");
    await user.keyboard(" ");
    expect(onPress).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId("go").getAttribute("data-variant")).toBe("primary");
  });

  // spec: UI-092
  test("a disabled button is skipped by Tab", async () => {
    const user = userEvent.setup();
    render(
      <>
        <Button data-testid="off" isDisabled>
          Off
        </Button>
        <Button data-testid="on">On</Button>
      </>,
    );
    await user.tab();
    expect(focused()).toBe("on");
  });
});

function CollapsibleCard() {
  const [expanded, setExpanded] = useState(false);
  return (
    <Card title="Tool call" tone="error" collapsible={{ expanded, onExpandedChange: setExpanded }} data-testid="card">
      <span data-testid="body">output</span>
    </Card>
  );
}

describe("Card", () => {
  // spec: UI-092
  test("a collapsible card toggles from the keyboard and reports aria-expanded", async () => {
    const user = userEvent.setup();
    render(<CollapsibleCard />);
    const toggle = screen.getByTestId("toggle");
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByTestId("body")).toBeNull();
    await user.tab();
    expect(focused()).toBe("toggle");
    await user.keyboard("{Enter}");
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByTestId("body").textContent).toBe("output");
    expect(screen.getByTestId("card").getAttribute("data-tone")).toBe("error");
  });

  test("renders actions next to the title", () => {
    render(<Card title="Repo" actions={<Button data-testid="act">Act</Button>} />);
    expect(screen.getByTestId("act")).toBeDefined();
  });
});

/** The panels whose effects run. */
const running = new Set<string>();

/** A text field whose effect runs while its panel shows. */
function Draft({ name }: { name: string }) {
  useEffect(() => {
    running.add(name);
    return () => {
      running.delete(name);
    };
  }, [name]);
  return <input aria-label={name} data-testid={`draft-${name}`} />;
}

describe("Tabs", () => {
  // spec: UI-092
  test("arrow keys move between tabs and show their panel", async () => {
    const user = userEvent.setup();
    render(
      <Tabs>
        <TabList aria-label="Panels">
          <Tab id="files" data-testid="tab-files">
            Files
          </Tab>
          <Tab id="log" data-testid="tab-log">
            Log
          </Tab>
        </TabList>
        <TabPanel id="files">files panel</TabPanel>
        <TabPanel id="log">log panel</TabPanel>
      </Tabs>,
    );
    await user.tab();
    expect(focused()).toBe("tab-files");
    await user.keyboard("{ArrowRight}");
    expect(focused()).toBe("tab-log");
    expect(screen.getByTestId("tab-log").getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel").textContent).toBe("log panel");
  });

  // spec: UI-402
  test("a kept panel stays mounted while another tab shows: hidden, inert, out of the Tab order, effects paused", async () => {
    const user = userEvent.setup();
    render(
      <Tabs>
        <TabList aria-label="Panels">
          <Tab id="files" data-testid="tab-files">
            Files
          </Tab>
          <Tab id="log" data-testid="tab-log">
            Log
          </Tab>
        </TabList>
        <TabPanel id="files" keepMounted data-testid="panel-files">
          <Draft name="files" />
        </TabPanel>
        <TabPanel id="log" keepMounted data-testid="panel-log">
          <Draft name="log" />
        </TabPanel>
      </Tabs>,
    );
    const panel = (name: string) => screen.getByTestId(`panel-${name}`);
    expect([panel("files").hidden, panel("log").hidden]).toEqual([false, true]);
    expect(panel("log").hasAttribute("inert")).toBe(true);
    expect([...running]).toEqual(["files"]);
    await user.type(screen.getByTestId("draft-files"), "unsaved");
    await user.click(screen.getByTestId("tab-log"));
    expect([panel("files").hidden, panel("log").hidden]).toEqual([true, false]);
    expect(panel("files").hasAttribute("inert")).toBe(true);
    expect(screen.getAllByRole("tabpanel").length).toBe(1);
    expect([...running]).toEqual(["log"]);
    // Tab from the selected tab goes into the shown panel, never into the hidden one.
    await user.tab();
    expect(focused()).toBe("draft-log");
    await user.click(screen.getByTestId("tab-files"));
    expect((screen.getByTestId("draft-files") as HTMLInputElement).value).toBe("unsaved");
    expect([...running].sort()).toEqual(["files"]);
  });
});

describe("ListBox", () => {
  // spec: UI-092
  test("arrow keys move the focus and Enter selects", async () => {
    const user = userEvent.setup();
    const onSelectionChange = mock((_keys: unknown) => undefined);
    render(
      <ListBox aria-label="Threads" selectionMode="single" onSelectionChange={onSelectionChange}>
        <ListBoxItem id="a" data-testid="a">
          A
        </ListBoxItem>
        <ListBoxItem id="b" data-testid="b">
          B
        </ListBoxItem>
      </ListBox>,
    );
    await user.tab();
    expect(focused()).toBe("a");
    await user.keyboard("{ArrowDown}");
    expect(focused()).toBe("b");
    await user.keyboard("{Enter}");
    expect(screen.getByTestId("b").getAttribute("aria-selected")).toBe("true");
    expect(onSelectionChange).toHaveBeenCalledTimes(1);
  });
});

describe("Menu", () => {
  // spec: UI-092
  test("opens from its trigger with the keyboard and runs the chosen action", async () => {
    const user = userEvent.setup();
    const onAction = mock((_key: Key) => undefined);
    render(
      <MenuTrigger>
        <Button data-testid="more">More</Button>
        <Popover>
          <Menu aria-label="Thread actions" onAction={onAction}>
            <MenuItem id="rename" data-testid="rename">
              Rename
            </MenuItem>
            <MenuItem id="archive" data-testid="archive">
              Archive
            </MenuItem>
          </Menu>
        </Popover>
      </MenuTrigger>,
    );
    await user.tab();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(focused()).toBe("rename"));
    await user.keyboard("{ArrowDown}");
    expect(focused()).toBe("archive");
    await user.keyboard("{Enter}");
    expect(onAction.mock.calls.map(([key]) => key)).toEqual(["archive"]);
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(focused()).toBe("more");
  });

  // spec: UI-092, UI-288
  test("opened with the mouse, the first ArrowDown lands on the first item, even after the pointer crossed one", async () => {
    const user = userEvent.setup();
    render(
      <MenuTrigger>
        <Button data-testid="more">More</Button>
        <Popover>
          <Menu aria-label="Thread actions">
            <MenuItem id="rename" data-testid="rename">
              Rename
            </MenuItem>
            <MenuItem id="archive" data-testid="archive">
              Archive
            </MenuItem>
            <MenuItem id="delete" data-testid="delete">
              Delete
            </MenuItem>
          </Menu>
        </Popover>
      </MenuTrigger>,
    );
    await user.click(screen.getByTestId("more"));
    await screen.findByRole("menu");
    // The pointer crosses the second item on its way out: React Aria keeps it focused, unseen.
    await user.hover(screen.getByTestId("archive"));
    await user.unhover(screen.getByTestId("archive"));
    await user.keyboard("{ArrowDown}");
    expect(focused()).toBe("rename");
    expect(screen.getByTestId("rename").hasAttribute("data-focus-visible")).toBe(true);
    await user.keyboard("{ArrowDown}");
    expect(focused()).toBe("archive");
  });
});

function ModeSelect({ onChange }: { onChange: (key: Key | null) => void }) {
  const [mode, setMode] = useState<Key | null>("yolo");
  return (
    <Select
      label="Approval"
      data-testid="select"
      items={[
        { id: "yolo", label: "Yolo" },
        { id: "ask", label: "Ask" },
      ]}
      selectedKey={mode}
      onSelectionChange={(key) => {
        setMode(key);
        onChange(key);
      }}
    />
  );
}

describe("Select", () => {
  // spec: UI-092
  test("opens with the keyboard and changes the selection", async () => {
    const user = userEvent.setup();
    const onChange = mock((_key: Key | null) => undefined);
    render(<ModeSelect onChange={onChange} />);
    const button = screen.getByRole("button");
    expect(button.textContent).toContain("Yolo");
    await user.tab();
    expect(document.activeElement === button).toBe(true);
    await user.keyboard("{ArrowDown}");
    const listbox = await screen.findByRole("listbox");
    expect(listbox.textContent).toContain("Ask");
    const option = listbox.querySelector<HTMLElement>('[data-testid="select-option"][data-key="ask"]');
    expect(option?.getAttribute("role")).toBe("option");
    if (option !== null) await user.click(option);
    expect(onChange).toHaveBeenCalledWith("ask");
    await waitFor(() => expect(screen.getByRole("button").textContent).toContain("Ask"));
  });
});

describe("text inputs", () => {
  // spec: UI-092
  test("TextField is labelled, reachable, typed into and shows its error", async () => {
    const user = userEvent.setup();
    const onChange = mock((_value: string) => undefined);
    render(
      <TextField
        label="Title"
        description="Shown in the sidebar"
        errorMessage="Too long"
        onChange={onChange}
        inputProps={{ "data-testid": "title" }}
      />,
    );
    await user.tab();
    expect(focused()).toBe("title");
    await user.keyboard("abc");
    expect(onChange).toHaveBeenLastCalledWith("abc");
    const input = screen.getByLabelText("Title");
    expect(input.getAttribute("data-testid")).toBe("title");
    expect(input.getAttribute("aria-invalid")).toBe("true");
    expect(screen.getByText("Too long")).toBeDefined();
  });

  // spec: UI-092
  test("TextArea takes multi-line text and exposes its element through inputRef", async () => {
    const user = userEvent.setup();
    const ref: { current: HTMLTextAreaElement | null } = { current: null };
    render(<TextArea label="Message" hideLabel inputRef={ref} inputProps={{ "data-testid": "composer" }} />);
    expect(ref.current?.tagName).toBe("TEXTAREA");
    act(() => ref.current?.focus());
    await user.keyboard("one{Shift>}{Enter}{/Shift}two");
    expect(ref.current?.value).toBe("one\ntwo");
    expect(screen.getByLabelText("Message") === screen.getByTestId("composer")).toBe(true);
  });

  // spec: UI-092
  test("SearchField clears on Escape", async () => {
    const user = userEvent.setup();
    const onChange = mock((_value: string) => undefined);
    render(<SearchField label="Filter" onChange={onChange} inputProps={{ "data-testid": "filter" }} />);
    await user.tab();
    await user.keyboard("abc");
    await user.keyboard("{Escape}");
    expect(onChange).toHaveBeenLastCalledWith("");
    expect((screen.getByTestId("filter") as HTMLInputElement).value).toBe("");
  });
});

describe("choices", () => {
  // spec: UI-092
  test("Checkbox toggles with Space", async () => {
    const user = userEvent.setup();
    const onChange = mock((_selected: boolean) => undefined);
    render(<Checkbox onChange={onChange}>Show archived</Checkbox>);
    await user.tab();
    await user.keyboard(" ");
    expect(onChange).toHaveBeenCalledWith(true);
  });

  // spec: UI-092
  test("RadioGroup moves the selection with arrow keys", async () => {
    const user = userEvent.setup();
    const onChange = mock((_value: string) => undefined);
    render(
      <RadioGroup label="Theme" defaultValue="light" onChange={onChange}>
        <Radio value="light">Light</Radio>
        <Radio value="dark">Dark</Radio>
      </RadioGroup>,
    );
    await user.tab();
    await user.keyboard("{ArrowDown}");
    expect(onChange).toHaveBeenCalledWith("dark");
  });
});

describe("Chip", () => {
  // spec: UI-092
  test("the remove button is named and reachable", async () => {
    const user = userEvent.setup();
    const onRemove = mock(() => undefined);
    render(
      <Chip onRemove={onRemove} removeLabel="Remove image">
        image.png
      </Chip>,
    );
    await user.tab();
    expect(focused()).toBe("remove");
    expect(screen.getByRole("button", { name: "Remove image" })).toBeDefined();
    await user.keyboard("{Enter}");
    expect(onRemove).toHaveBeenCalledTimes(1);
  });
});

describe("Disclosure", () => {
  // spec: UI-092
  test("expands its panel from the keyboard", async () => {
    const user = userEvent.setup();
    const onExpandedChange = mock((_expanded: boolean) => undefined);
    render(
      <Disclosure title="Thinking" onExpandedChange={onExpandedChange}>
        <span>thoughts</span>
      </Disclosure>,
    );
    const toggle = screen.getByTestId("toggle");
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    await user.tab();
    await user.keyboard("{Enter}");
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(onExpandedChange).toHaveBeenCalledWith(true);
  });
});

describe("Tooltip", () => {
  // spec: UI-092
  test("shows its text when the trigger gets keyboard focus", async () => {
    const user = userEvent.setup();
    render(
      <TooltipTrigger delay={0}>
        <Button data-testid="trigger">?</Button>
        <Tooltip>Help text</Tooltip>
      </TooltipTrigger>,
    );
    await user.tab();
    const tooltip = await screen.findByRole("tooltip");
    expect(tooltip.textContent).toBe("Help text");
  });
});

describe("status", () => {
  // spec: UI-093
  test("Progress reports its value or is indeterminate", () => {
    render(
      <>
        <Progress label="Upload" value={30} max={60} data-testid="determinate" />
        <Progress label="Working" data-testid="busy" />
      </>,
    );
    const determinate = screen.getByTestId("determinate");
    expect(determinate.getAttribute("aria-valuenow")).toBe("30");
    expect(determinate.getAttribute("aria-valuemax")).toBe("60");
    expect(screen.getByTestId("busy").hasAttribute("aria-valuenow")).toBe(false);
    expect(screen.getByRole("progressbar", { name: "Working" }) === screen.getByTestId("busy")).toBe(true);
  });

  // spec: UI-092
  test("Log is a focusable log that follows the end", () => {
    const { rerender } = render(<Log lines={["one"]} data-testid="log" aria-label="Output" />);
    const log = screen.getByTestId("log");
    expect(log.getAttribute("role")).toBe("log");
    expect(log.tabIndex).toBe(0);
    Object.defineProperty(log, "scrollHeight", { configurable: true, value: 500 });
    rerender(<Log lines={["one", "two"]} data-testid="log" aria-label="Output" />);
    expect(log.scrollTop).toBe(500);
    expect(log.textContent).toBe("onetwo");
  });

  // spec: UI-092
  test("LiveRegion announces politely", () => {
    const { rerender } = render(<LiveRegion message="" />);
    const region = screen.getByRole("status");
    expect(region.getAttribute("aria-live")).toBe("polite");
    rerender(<LiveRegion message="Thread archived" />);
    expect(region.textContent).toBe("Thread archived");
  });
});
