/** L4: the full-screen sheet (UI-461) with its sections and touch rows (UI-462). */

import { afterEach, describe, expect, mock, test } from "bun:test";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Sheet, SheetRow, SheetSection } from "../src/index.ts";

afterEach(cleanup);

function Example({ onClose, onRow }: { onClose: () => void; onRow: (id: string) => void }) {
  return (
    <Sheet title="Commands" closeLabel="Close" onClose={onClose} search={<input data-testid="search" />}>
      <SheetSection title="Threads" data-testid="section">
        <SheetRow name=":new" label="New thread" data-testid="row" onPress={() => onRow("new")} />
        <SheetRow name=":theme" label="Theme" more data-testid="row" onPress={() => onRow("theme")} />
      </SheetSection>
    </Sheet>
  );
}

describe("Sheet", () => {
  // spec: UI-461, UI-466
  test("a modal dialog named by its title, with a close button, the search slot and the sections", async () => {
    const onClose = mock(() => {});
    render(<Example onClose={onClose} onRow={() => {}} />);
    const sheet = screen.getByRole("dialog", { name: "Commands" });
    expect(sheet.className).toBe("omfp-sheet");
    expect(sheet.querySelector(".omfp-sheet__title")?.textContent).toBe("Commands");
    expect(within(sheet).getAllByTestId("search").length).toBe(1);
    const section = screen.getByTestId("section");
    expect([section.querySelector("h3")?.textContent, section.getAttribute("aria-labelledby") !== null]).toEqual([
      "Threads",
      true,
    ]);
    await userEvent.setup().click(screen.getByRole("button", { name: "Close" }));
    expect(onClose.mock.calls.length).toBe(1);
  });

  // spec: UI-461
  test("Escape closes it", async () => {
    const onClose = mock(() => {});
    render(<Example onClose={onClose} onRow={() => {}} />);
    screen.getAllByTestId("row")[0]?.focus();
    await userEvent.setup().keyboard("{Escape}");
    expect(onClose.mock.calls.length).toBe(1);
  });

  // spec: UI-462, UI-466
  test("a row is a button: its label, then its name; one taking more shows it", async () => {
    const onRow = mock((_id: string) => {});
    render(<Example onClose={() => {}} onRow={onRow} />);
    const rows = screen.getAllByTestId("row");
    expect(
      rows.map((row) => [
        row.tagName,
        [...row.querySelectorAll(".omfp-sheet__text > span")].map((line) => `${line.className}: ${line.textContent}`),
        row.dataset.more ?? null,
        row.querySelectorAll("svg").length,
      ]),
    ).toEqual([
      ["BUTTON", ["omfp-sheet__label: New thread", "omfp-sheet__name: :new"], null, 0],
      ["BUTTON", ["omfp-sheet__label: Theme", "omfp-sheet__name: :theme"], "true", 1],
    ]);
    await userEvent.setup().click(rows[1] ?? document.body);
    expect(onRow.mock.calls).toEqual([["theme"]]);
  });

  // spec: UI-461, UI-466
  test("inline, it renders in place without the modal overlay (the gallery)", () => {
    render(
      <Sheet inline title="Commands" closeLabel="Close">
        <p>empty</p>
      </Sheet>,
    );
    expect(document.querySelectorAll(".omfp-modal-overlay").length).toBe(0);
    expect(screen.getByRole("dialog", { name: "Commands" }).closest(".omfp-sheet-modal") === null).toBe(false);
  });
});
