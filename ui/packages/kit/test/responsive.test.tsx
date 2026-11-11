/** L4: the kit's parts of the narrow and phone frame (UI-208): splitter, strip and area bar. */

import { afterEach, describe, expect, mock, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AreaBar, Splitter, ThreadStrip } from "../src/index.ts";

afterEach(cleanup);

describe("Splitter", () => {
  // spec: UI-208, UI-206
  test("a focusable separator with its orientation, name and the share before it", () => {
    render(<Splitter orientation="vertical" label="Resize the side" value={74.6} data-testid="bar" />);
    const bar = screen.getByRole("separator", { name: "Resize the side" });
    expect([
      bar.className,
      bar.tabIndex,
      bar.getAttribute("aria-orientation"),
      bar.getAttribute("aria-valuemin"),
      bar.getAttribute("aria-valuemax"),
      bar.getAttribute("aria-valuenow"),
    ]).toEqual(["omfp-splitter", 0, "vertical", "0", "100", "75"]);
  });
});

describe("ThreadStrip", () => {
  // spec: UI-208, UI-201
  test("the brand, a toggle telling whether the column is open, then one labelled state mark per thread", async () => {
    const onToggle = mock(() => {});
    const onSelect = mock((_id: string) => {});
    render(
      <ThreadStrip
        aria-label="Threads column"
        open={false}
        onToggle={onToggle}
        toggleLabel="Show threads"
        controls="column"
        threads={[
          { id: "a", title: "Asks", state: "awaiting" },
          { id: "r", title: "Runs", state: "running", selected: true },
        ]}
        onSelect={onSelect}
      />,
    );
    const user = userEvent.setup();
    const toggle = screen.getByTestId("left-toggle");
    expect([
      toggle.getAttribute("aria-label"),
      toggle.getAttribute("aria-expanded"),
      toggle.getAttribute("aria-controls"),
    ]).toEqual(["Show threads", "false", "column"]);
    expect(
      screen
        .getAllByTestId("strip-thread")
        .map((button) => [button.getAttribute("aria-label"), button.dataset.state, button.dataset.selected ?? null]),
    ).toEqual([
      ["Asks", "awaiting", null],
      ["Runs", "running", "true"],
    ]);
    await user.click(toggle);
    await user.click(screen.getAllByTestId("strip-thread")[0] ?? document.body);
    expect([onToggle.mock.calls.length, onSelect.mock.calls]).toEqual([1, [["a"]]]);
  });
});

describe("AreaBar", () => {
  // spec: UI-208, UI-204
  test("one button per area, icon over label; the shown one is pressed", async () => {
    const onSelect = mock((_id: string) => {});
    render(
      <AreaBar
        label="Areas"
        items={[
          { id: "threads", label: "Threads", icon: "panel-left" },
          { id: "chat", label: "Chat", icon: "message-square" },
        ]}
        selected="chat"
        onSelect={onSelect}
      />,
    );
    const buttons = screen.getAllByTestId("area-button");
    expect(buttons.map((button) => [button.textContent, button.getAttribute("aria-pressed")])).toEqual([
      ["Threads", "false"],
      ["Chat", "true"],
    ]);
    expect(buttons.map((button) => button.querySelectorAll("svg").length)).toEqual([1, 1]);
    expect(screen.getByRole("navigation", { name: "Areas" }).className).toBe("omfp-areabar");
    await userEvent.setup().click(buttons[0] ?? document.body);
    expect(onSelect.mock.calls).toEqual([["threads"]]);
  });
});
