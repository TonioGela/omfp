import { afterEach, describe, expect, mock, test } from "bun:test";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { Button, ConfirmDialog, Dialog, DialogTrigger, TextField } from "../src/index.ts";

afterEach(cleanup);

/** The test id of the focused element (short failure output). */
function focused(): string | null {
  return document.activeElement?.getAttribute("data-testid") ?? document.activeElement?.tagName ?? null;
}

function Example() {
  return (
    <>
      <Button data-testid="before">Before</Button>
      <DialogTrigger>
        <Button data-testid="open">Rename</Button>
        <Dialog title="Rename thread" data-testid="dialog">
          {({ close }) => (
            <>
              <TextField label="Title" inputProps={{ "data-testid": "title" }} autoFocus />
              <Button data-testid="save" onPress={close}>
                Save
              </Button>
            </>
          )}
        </Dialog>
      </DialogTrigger>
    </>
  );
}

describe("Dialog", () => {
  // spec: UI-092
  test("opens from the keyboard, traps Tab inside, closes on Escape and restores focus", async () => {
    const user = userEvent.setup();
    render(<Example />);
    await user.tab();
    await user.tab();
    expect(focused()).toBe("open");
    await user.keyboard("{Enter}");

    const dialog = await screen.findByTestId("dialog");
    expect(dialog.getAttribute("role")).toBe("dialog");
    expect(screen.getByRole("dialog", { name: "Rename thread" })).toBe(dialog);
    await waitFor(() => expect(focused()).toBe("title"));

    await user.tab();
    expect(focused()).toBe("save");
    await user.tab();
    expect(focused()).toBe("title");
    await user.tab();
    expect(focused()).toBe("save");
    await user.tab({ shift: true });
    expect(focused()).toBe("title");
    await user.tab({ shift: true });
    expect(dialog.contains(document.activeElement)).toBe(true);

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByTestId("dialog")).toBeNull());
    expect(focused()).toBe("open");
  });

  // spec: UI-092
  test("a controlled dialog closes through its render-prop close", async () => {
    const user = userEvent.setup();
    const onOpenChange = mock((_open: boolean) => undefined);
    render(
      <Dialog title="Info" isOpen onOpenChange={onOpenChange}>
        {({ close }) => (
          <Button data-testid="ok" onPress={close}>
            OK
          </Button>
        )}
      </Dialog>,
    );
    await user.click(screen.getByTestId("ok"));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});

function Confirm({ onConfirm, tone }: { onConfirm: () => void; tone?: "danger" }) {
  const [open, setOpen] = useState(true);
  return (
    <ConfirmDialog
      isOpen={open}
      onOpenChange={setOpen}
      title="Delete thread?"
      message="This cannot be undone."
      confirmLabel="Delete"
      onConfirm={onConfirm}
      {...(tone === undefined ? {} : { tone })}
    />
  );
}

describe("ConfirmDialog", () => {
  // spec: UI-092
  test("is an alert dialog; confirm calls onConfirm and closes", async () => {
    const user = userEvent.setup();
    const onConfirm = mock(() => undefined);
    render(<Confirm onConfirm={onConfirm} />);
    const dialog = screen.getByTestId("confirm-dialog");
    expect(dialog.getAttribute("role")).toBe("alertdialog");
    await waitFor(() => expect(focused()).toBe("confirm"));
    await user.keyboard("{Enter}");
    expect(onConfirm).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByTestId("confirm-dialog")).toBeNull());
  });

  // spec: UI-092
  test("danger focuses Cancel first; cancelling never confirms", async () => {
    const user = userEvent.setup();
    const onConfirm = mock(() => undefined);
    render(<Confirm onConfirm={onConfirm} tone="danger" />);
    await waitFor(() => expect(focused()).toBe("cancel"));
    expect(screen.getByTestId("confirm").getAttribute("data-variant")).toBe("danger");
    await user.keyboard("{Enter}");
    await waitFor(() => expect(screen.queryByTestId("confirm-dialog")).toBeNull());
    expect(onConfirm).not.toHaveBeenCalled();
  });

  test("test ids are overridable", () => {
    render(
      <ConfirmDialog
        isOpen
        onOpenChange={() => undefined}
        title="Archive?"
        message="Archive it."
        confirmLabel="Archive"
        onConfirm={() => undefined}
        testId="archive-dialog"
        confirmTestId="archive-confirm"
        cancelTestId="archive-cancel"
      />,
    );
    expect(screen.getByTestId("archive-dialog")).toBeDefined();
    expect(screen.getByTestId("archive-confirm").textContent).toBe("Archive");
    expect(screen.getByTestId("archive-cancel").textContent).toBe("Cancel");
  });
});
