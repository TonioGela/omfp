import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import {
  Badge,
  Button,
  Card,
  Checkbox,
  Chip,
  Code,
  Disclosure,
  IconButton,
  Kbd,
  ListBox,
  ListBoxItem,
  LiveRegion,
  Log,
  Markdown,
  Menu,
  MenuItem,
  MenuSection,
  Progress,
  Radio,
  RadioGroup,
  SearchField,
  Select,
  Separator,
  Tab,
  TabList,
  TabPanel,
  Tabs,
  TextArea,
  TextField,
  Toolbar,
} from "../src/index.ts";

afterEach(cleanup);

/** Common forwarded props: every component puts them on its root element. */
const FORWARDED = { "data-testid": "subject", id: "subject-id", className: "extra" } as const;

interface Case {
  readonly name: string;
  readonly element: ReactElement;
  /** The kit class the root carries. */
  readonly base: string;
  /** The root's ARIA role, when it has one. */
  readonly role?: string;
  /** Fields put `id` on their input (so labels and `aria-*` refer to it), not on the wrapper. */
  readonly idOnControl?: true;
}

const CASES: readonly Case[] = [
  { name: "Button", element: <Button {...FORWARDED}>Go</Button>, base: "omfp-button", role: "button" },
  {
    name: "IconButton",
    element: <IconButton {...FORWARDED} label="Close" icon="×" />,
    base: "omfp-icon-button",
    role: "button",
  },
  { name: "Card", element: <Card {...FORWARDED} title="Title" />, base: "omfp-card" },
  {
    name: "Tabs",
    element: (
      <Tabs {...FORWARDED}>
        <TabList aria-label="Views">
          <Tab id="a">A</Tab>
        </TabList>
        <TabPanel id="a">Panel</TabPanel>
      </Tabs>
    ),
    base: "omfp-tabs",
  },
  {
    name: "ListBox",
    element: (
      <ListBox {...FORWARDED} aria-label="Items">
        <ListBoxItem id="one">One</ListBoxItem>
      </ListBox>
    ),
    base: "omfp-listbox",
    role: "listbox",
  },
  {
    name: "Menu",
    element: (
      <Menu {...FORWARDED} aria-label="Actions">
        <MenuSection title="Thread">
          <MenuItem id="rename">Rename</MenuItem>
        </MenuSection>
        <Separator />
        <MenuItem id="delete">Delete</MenuItem>
      </Menu>
    ),
    base: "omfp-menu",
    role: "menu",
  },
  { name: "TextField", element: <TextField {...FORWARDED} label="Name" />, base: "omfp-field", idOnControl: true },
  { name: "TextArea", element: <TextArea {...FORWARDED} label="Message" />, base: "omfp-field", idOnControl: true },
  {
    name: "SearchField",
    element: <SearchField {...FORWARDED} label="Filter" />,
    base: "omfp-search",
    idOnControl: true,
  },
  {
    name: "Select",
    element: (
      <Select
        {...FORWARDED}
        label="Mode"
        items={[{ id: "yolo", label: "Yolo" }]}
        selectedKey="yolo"
        onSelectionChange={() => undefined}
      />
    ),
    base: "omfp-select",
    idOnControl: true,
  },
  {
    name: "Checkbox",
    element: <Checkbox {...FORWARDED}>Pinned</Checkbox>,
    base: "omfp-checkbox",
    idOnControl: true,
  },
  {
    name: "RadioGroup",
    element: (
      <RadioGroup {...FORWARDED} label="Scheme">
        <Radio value="light">Light</Radio>
      </RadioGroup>
    ),
    base: "omfp-radio-group",
    role: "radiogroup",
  },
  { name: "Badge", element: <Badge {...FORWARDED}>3</Badge>, base: "omfp-badge" },
  { name: "Chip", element: <Chip {...FORWARDED}>file.png</Chip>, base: "omfp-chip" },
  { name: "Kbd", element: <Kbd {...FORWARDED}>j</Kbd>, base: "omfp-kbd" },
  {
    name: "Progress",
    element: <Progress {...FORWARDED} label="Loading" />,
    base: "omfp-progress",
    role: "progressbar",
  },
  { name: "Log", element: <Log {...FORWARDED} lines={["a", "b"]} />, base: "omfp-log", role: "log" },
  { name: "Markdown", element: <Markdown {...FORWARDED} text="*hi*" />, base: "omfp-markdown" },
  { name: "Code", element: <Code {...FORWARDED} code="ls" />, base: "omfp-code" },
  {
    name: "LiveRegion",
    element: <LiveRegion {...FORWARDED} message="Saved" />,
    base: "omfp-visually-hidden",
    role: "status",
  },
  {
    name: "Toolbar",
    element: (
      <Toolbar {...FORWARDED} aria-label="Format">
        <Button>B</Button>
      </Toolbar>
    ),
    base: "omfp-toolbar",
    role: "toolbar",
  },
  { name: "Disclosure", element: <Disclosure {...FORWARDED} title="Thinking" />, base: "omfp-disclosure" },
];

describe("kit components", () => {
  // spec: UI-093
  test.each(CASES.map((c) => [c.name, c] as const))("%s renders and forwards id, className, data-*", (_name, c) => {
    render(c.element);
    const root = screen.getByTestId("subject");
    const withId = document.getElementById("subject-id");
    expect(c.idOnControl === true ? withId !== root && root.contains(withId) : withId === root).toBe(true);
    expect(root.classList.contains(c.base)).toBe(true);
    expect(root.classList.contains("extra")).toBe(true);
    if (c.role !== undefined) {
      expect(root.getAttribute("role") ?? (root.tagName === "BUTTON" ? "button" : null)).toBe(c.role);
    }
  });
});
