/**
 * Layout editing in the frame (UI-103, UI-104): this window's edit mode, the frame bar's "Edit layout" toggle and
 * "Add widget" picker, and the writer that turns a layout edit into the manifest `views.set` writes. The window then
 * renders the manifest the daemon pushes back. On a `views.json` view init files changed, the edit goes to the nodes
 * `views.json` holds (UI-269), and the init files change the new file again.
 */

import { Button, Dialog, Toolbar } from "@omfp/kit";
import type { ViewsState } from "@omfp/protocol";
import { type ReactNode, useCallback, useId, useState } from "react";
import { useCall, useReportError, useWindowState, windowKey } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { type LayoutEdit, manifestToWrite } from "../view/edit.ts";
import { toBase } from "../view/origins.ts";
import type { WidgetRegistry } from "../view/resolve.ts";
import type { ResolvedView } from "../view/views.ts";

const text = strings.shell.layout;

/** Whether this window edits its view's layout (window-local, UI-103). */
export const LAYOUT_EDIT = windowKey<boolean>("layoutEdit", false);

/**
 * Writes `edit` on the view `viewId` of `workspaceId` (`views.set`); failures and refused edits go to the frame's
 * error list. `shown` is that view as the window shows it: with `origins` (UI-269), the edit's paths are mapped onto
 * `views.json`'s layout, and edits of nodes an init file added are refused. Resolves to whether the manifest was
 * written.
 */
export function useLayoutWriter(
  workspaceId: string | undefined,
  views: ViewsState | undefined,
  viewId: string,
  shown: ResolvedView | undefined,
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
      const based =
        shown?.origins === undefined ? { ok: true as const, value: edit } : toBase(shown.origins, shown.tree, edit);
      const manifest = based.ok ? manifestToWrite(views, viewId, based.value) : based;
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
    [call, report, workspaceId, views, viewId, shown],
  );
}

/**
 * "Edit layout": toggles this window's edit mode; only with a current workspace, whose manifest edits write. With a
 * `locked` reason (UI-269: a TSX view, a view an init file added) it is off and says why.
 */
export function LayoutEditToggle({
  editing,
  enabled,
  locked,
}: {
  editing: boolean;
  enabled: boolean;
  locked?: string | undefined;
}): ReactNode {
  const [, setEditing] = useWindowState(LAYOUT_EDIT);
  const note = useId();
  return (
    <>
      <Button
        size="sm"
        variant={editing ? "primary" : "quiet"}
        aria-pressed={editing}
        isDisabled={!enabled || locked !== undefined}
        {...(locked === undefined ? {} : { "aria-describedby": note })}
        data-testid="layout-edit"
        onPress={() => setEditing((previous) => !previous)}
      >
        {text.edit}
      </Button>
      {locked !== undefined && (
        <p id={note} className="omfp-frame__note" data-testid="layout-edit-note">
          {locked}
        </p>
      )}
    </>
  );
}

interface AddWidgetProps {
  readonly registry: WidgetRegistry;
  readonly views: ViewsState | undefined;
  readonly onAdd: (reference: string) => void;
}

/**
 * "Add widget" and its picker (UI-104): built-ins, the workspace's widgets and the user library's, one vertical
 * toolbar (the arrow keys move between them, UI-092).
 */
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
          <Toolbar orientation="vertical" className="omfp-widget-picker" aria-label={text.picker}>
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
          </Toolbar>
        )}
      </Dialog>
    </>
  );
}
