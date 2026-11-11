/**
 * Layout editing in the frame (UI-103, UI-104): this window's edit mode, the frame bar's "Edit layout" toggle and
 * "Add widget" picker, and the writer that turns a layout edit into the manifest `views.set` writes. The window then
 * renders the manifest the daemon pushes back.
 */

import { Button, Dialog } from "@omfp/kit";
import type { ViewsState } from "@omfp/protocol";
import { type ReactNode, useCallback, useState } from "react";
import { useCall, useReportError, useWindowState, windowKey } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { type LayoutEdit, manifestToWrite } from "../view/edit.ts";
import type { WidgetRegistry } from "../view/resolve.ts";

const text = strings.shell.layout;

/** Whether this window edits its view's layout (window-local, UI-103). */
export const LAYOUT_EDIT = windowKey<boolean>("layoutEdit", false);

/**
 * Writes `edit` on the view `viewId` of `workspaceId` (`views.set`); failures and refused edits go to the frame's
 * error list. Resolves to whether the manifest was written.
 */
export function useLayoutWriter(
  workspaceId: string | undefined,
  views: ViewsState | undefined,
  viewId: string,
): (edit: LayoutEdit) => Promise<boolean> {
  const call = useCall();
  const report = useReportError();
  return useCallback(
    async (edit: LayoutEdit) => {
      if (workspaceId === undefined) {
        report(text.noWorkspace);
        return false;
      }
      // Before the views topic answers, an existing views.json is unknown and must not be overwritten.
      if (views === undefined) {
        report(text.loading);
        return false;
      }
      const manifest = manifestToWrite(views, viewId, edit);
      if (!manifest.ok) {
        report(manifest.error);
        return false;
      }
      try {
        await call("views.set", { workspaceId, manifest: manifest.value });
        return true;
      } catch (error) {
        report(error);
        return false;
      }
    },
    [call, report, workspaceId, views, viewId],
  );
}

/** "Edit layout": toggles this window's edit mode; only with a current workspace, whose manifest edits write. */
export function LayoutEditToggle({ editing, enabled }: { editing: boolean; enabled: boolean }): ReactNode {
  const [, setEditing] = useWindowState(LAYOUT_EDIT);
  return (
    <Button
      size="sm"
      variant={editing ? "primary" : "quiet"}
      aria-pressed={editing}
      isDisabled={!enabled}
      data-testid="layout-edit"
      onPress={() => setEditing((previous) => !previous)}
    >
      {text.edit}
    </Button>
  );
}

interface AddWidgetProps {
  readonly registry: WidgetRegistry;
  readonly views: ViewsState | undefined;
  readonly onAdd: (reference: string) => void;
}

/** "Add widget" and its picker (UI-104): built-ins, the workspace's widgets and the user library's. */
export function AddWidget({ registry, views, onAdd }: AddWidgetProps): ReactNode {
  const [open, setOpen] = useState(false);
  const folders = views?.widgets ?? [];
  const groups = [
    { title: text.builtin, references: [...registry.keys()] },
    {
      title: text.workspace,
      references: folders.filter((widget) => widget.source === "workspace").map((widget) => `./widgets/${widget.name}`),
    },
    {
      title: text.user,
      references: folders.filter((widget) => widget.source === "user").map((widget) => `user:${widget.name}`),
    },
  ];
  return (
    <>
      <Button size="sm" variant="quiet" data-testid="widget-add" onPress={() => setOpen(true)}>
        {text.add}
      </Button>
      <Dialog title={text.picker} isOpen={open} onOpenChange={setOpen} isDismissable data-testid="widget-picker">
        {({ close }) => (
          <div className="omfp-widget-picker">
            {groups.map(({ title, references }) => (
              <section key={title} className="omfp-widget-picker__group" aria-label={title}>
                <h3 className="omfp-widget-picker__title">{title}</h3>
                {references.length === 0 ? (
                  <p className="omfp-widget-picker__empty">{text.noWidget}</p>
                ) : (
                  <ul className="omfp-widget-picker__list">
                    {references.map((reference) => (
                      <li key={reference}>
                        <Button
                          size="sm"
                          variant="quiet"
                          data-testid="widget-option"
                          data-widget={reference}
                          onPress={() => {
                            onAdd(reference);
                            close();
                          }}
                        >
                          {reference}
                        </Button>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            ))}
          </div>
        )}
      </Dialog>
    </>
  );
}
