/**
 * Files in a view without a `main` slot (UI-115): a panel floating over the view holding the open files as tabs, like
 * the `main` slot's (selected tab, each tab's own `×`, middle click and Delete, closing selects the neighbour,
 * window-local; UI-365, UI-366). The panel moves by its grip and resizes by its corner (pointer or arrow keys),
 * minimises to its bar, and keeps its place and state in this window. Esc closes the selected file (an Esc the file
 * editor kept for itself does not count).
 */

import { IconButton, Tab, TabList, TabPanel, Tabs } from "@omfp/kit";
import type { ViewsState } from "@omfp/protocol";
import type { KeyboardEvent, ReactNode } from "react";
import { FLOATING_FILES, fileName, fileTabId, movePanel, resizePanel, useFileTabs } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { FILE_WIDGET, fileProps } from "../view/layout.ts";
import { OpenedTabLabel, openedTabBarHandlers } from "../view/opened-tab.tsx";
import type { WidgetRegistry } from "../view/resolve.ts";
import { WidgetSlot } from "../view/slot.tsx";
import { useFloatingBox } from "./floating-box.ts";

const text = strings.shell.files;

export interface FloatingFileProps {
  readonly workspaceId: string | undefined;
  readonly views: ViewsState | undefined;
  readonly registry: WidgetRegistry;
}

export function FloatingFile({ workspaceId, views, registry }: FloatingFileProps): ReactNode {
  const files = useFileTabs(workspaceId);
  const { panel, setPanel, root, handle, style } = useFloatingBox(FLOATING_FILES);
  const first = files.tabs[0];
  if (first === undefined) {
    return null;
  }
  const selected = files.tabs.find((tab) => fileTabId(tab.path) === files.active) ?? first;
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && !event.defaultPrevented) {
      event.preventDefault();
      files.close(selected.path);
    }
  };
  const bar = (tabs: ReactNode) => (
    <div className="omfp-floating-file__bar" {...openedTabBarHandlers(files.closeTab)}>
      <button
        type="button"
        className="omfp-floating-file__grip omfp-focus-ring"
        aria-label={text.move}
        title={text.move}
        data-testid="floating-file-move"
        {...handle(movePanel)}
      >
        ⠿
      </button>
      {tabs}
      <IconButton
        label={panel.minimised ? text.restore : text.minimise}
        icon={panel.minimised ? "□" : "–"}
        onPress={() => setPanel((now) => ({ ...now, minimised: !now.minimised }))}
        data-testid="floating-file-minimise"
      />
    </div>
  );
  return (
    <section
      ref={root}
      className="omfp-floating-file"
      aria-label={text.panel(files.tabs.length)}
      data-testid="floating-file"
      data-minimised={panel.minimised ? "" : undefined}
      style={style}
      onKeyDown={onKeyDown}
    >
      {panel.minimised ? (
        bar(<span className="omfp-floating-file__title">{text.panel(files.tabs.length)}</span>)
      ) : (
        <>
          <Tabs
            className="omfp-floating-file__tabs"
            selectedKey={fileTabId(selected.path)}
            onSelectionChange={(key) => files.select(String(key))}
          >
            {bar(
              <TabList aria-label={text.tabs} className="omfp-floating-file__list">
                {files.tabs.map((tab) => (
                  <Tab
                    key={fileTabId(tab.path)}
                    id={fileTabId(tab.path)}
                    data-testid="floating-file-tab"
                    data-file={tab.path}
                    data-tab={fileTabId(tab.path)}
                  >
                    <OpenedTabLabel
                      id={fileTabId(tab.path)}
                      title={fileName(tab.path)}
                      tooltip={tab.path}
                      kind="file"
                      onClose={files.closeTab}
                    />
                  </Tab>
                ))}
              </TabList>,
            )}
            {files.tabs.map((tab) => (
              <TabPanel key={fileTabId(tab.path)} id={fileTabId(tab.path)} className="omfp-floating-file__body">
                <WidgetSlot
                  reference={FILE_WIDGET}
                  props={fileProps(tab)}
                  path={fileTabId(tab.path)}
                  workspaceId={workspaceId}
                  views={views}
                  registry={registry}
                />
              </TabPanel>
            ))}
          </Tabs>
          <button
            type="button"
            className="omfp-floating-file__resize omfp-focus-ring"
            aria-label={text.resize}
            title={text.resize}
            data-testid="floating-file-resize"
            {...handle(resizePanel)}
          />
        </>
      )}
    </section>
  );
}
