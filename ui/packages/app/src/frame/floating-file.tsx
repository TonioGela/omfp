/**
 * Files in a view without a `main` slot (UI-115): the selected file in a panel floating over the view, one at a
 * time, closed with its button or Esc (an Esc the file editor kept for itself does not count).
 */

import { IconButton } from "@omfp/kit";
import type { ViewsState } from "@omfp/protocol";
import type { KeyboardEvent, ReactNode } from "react";
import { fileName, fileTabId, useFileTabs } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { FILE_WIDGET, fileProps } from "../view/layout.ts";
import type { WidgetRegistry } from "../view/resolve.ts";
import { WidgetSlot } from "../view/slot.tsx";

const text = strings.shell.files;

export interface FloatingFileProps {
  readonly workspaceId: string | undefined;
  readonly views: ViewsState | undefined;
  readonly registry: WidgetRegistry;
}

export function FloatingFile({ workspaceId, views, registry }: FloatingFileProps): ReactNode {
  const files = useFileTabs(workspaceId);
  const tab = files.tabs.find((candidate) => fileTabId(candidate.path) === files.active);
  if (tab === undefined) {
    return null;
  }
  const close = () => {
    files.close(tab.path);
    files.select(undefined);
  };
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && !event.defaultPrevented) {
      event.preventDefault();
      close();
    }
  };
  return (
    <section
      className="omfp-floating-file"
      aria-label={text.panel(tab.path)}
      data-testid="floating-file"
      onKeyDown={onKeyDown}
    >
      <div className="omfp-floating-file__bar">
        <span className="omfp-floating-file__title">{fileName(tab.path)}</span>
        <IconButton label={text.close(fileName(tab.path))} icon="×" onPress={close} data-testid="floating-file-close" />
      </div>
      <div className="omfp-floating-file__body">
        <WidgetSlot
          reference={FILE_WIDGET}
          props={fileProps(tab)}
          path={fileTabId(tab.path)}
          workspaceId={workspaceId}
          views={views}
          registry={registry}
        />
      </div>
    </section>
  );
}
