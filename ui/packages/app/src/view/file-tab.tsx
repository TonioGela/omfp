/**
 * A file tab's label and the ways to close it (UI-365, UI-366), shared by the `main` slot's tabs and the floating file
 * panel: each tab carries its own `×`, a middle click on a tab closes it (its press never starts the browser's
 * autoscroll), Delete closes the focused tab. None selects the tab first.
 */

import { Icon } from "@omfp/kit";
import type { KeyboardEvent, MouseEvent, ReactNode } from "react";
import { fileName } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";

const text = strings.shell.files;

/** The file of the tab an event happened in (tabs carry `data-file`). */
function fileOf(event: MouseEvent | KeyboardEvent): string | undefined {
  return (event.target as Element).closest<HTMLElement>("[data-file]")?.dataset.file;
}

/** Handlers for the element around a file tab list: middle click and Delete close the tab they target. */
export function fileTabBarHandlers(close: (path: string) => void) {
  const middle = (event: MouseEvent) => {
    const path = event.button === 1 ? fileOf(event) : undefined;
    if (path !== undefined) {
      event.preventDefault();
      if (event.type === "auxclick") {
        close(path);
      }
    }
  };
  return {
    onMouseDown: middle,
    onAuxClick: middle,
    onKeyDown: (event: KeyboardEvent) => {
      const path = event.key === "Delete" ? fileOf(event) : undefined;
      if (path !== undefined) {
        event.preventDefault();
        close(path);
      }
    },
  };
}

/** The name of a file tab and its own `×`; the `×` is a pointer target (Delete is the keyboard way). */
export function FileTabLabel({ path, onClose }: { path: string; onClose: (path: string) => void }): ReactNode {
  return (
    <>
      <span title={path}>{fileName(path)}</span>
      <span
        className="omfp-file-tab__close"
        aria-hidden="true"
        title={text.close(fileName(path))}
        data-testid="file-tab-close"
        // The tab selects itself on press: the `×` closes without selecting.
        onPointerDown={(event) => event.stopPropagation()}
        onMouseDown={(event) => event.stopPropagation()}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          onClose(path);
        }}
      >
        <Icon name="x" />
      </span>
    </>
  );
}
