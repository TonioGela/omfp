/**
 * The thin frame around the view (docs/specs/08-agent-control.md): connection banner (UI-003), the left column of
 * workspaces and threads (UI-100), the view of the current workspace with its switcher, error banner and layout
 * editing (UI-100..104, AGT-011), the keymap, `:` command line (UI-095) and help panel (UI-198), the safety net for
 * pending interactions (AGT-015), floating panels (UI-230), notices and notifications (UI-096, UI-192), the live region
 * (UI-092), dialogs, the files the agent opens (DMN-060), and theme/density on `<html>` (UI-091, UI-094). Everything
 * else is a widget of the view.
 */

import { Select, useWidthClass } from "@omfp/kit";
import { type ViewsState, viewsTopic } from "@omfp/protocol";
import { type ReactNode, type RefObject, useEffect, useMemo, useRef } from "react";
import { createKeyHandler } from "../keymap/handler.ts";
import { type Binding, DEFAULT_BINDINGS } from "../keymap/keymap.ts";
import { KeymapError, useBindings } from "../keymap/workspace.tsx";
import {
  ACTIVE_VIEW,
  DRAFT_THREAD,
  EDITOR_MODE,
  SELECTED_THREAD,
  useHost,
  useSettings,
  useSnapshot,
  useThreadSummary,
  useViews,
  useWindowState,
} from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { useAppearance, useUserTheme } from "../theme/theme.ts";
import { chatCell, containsWidget, hasMainSlot } from "../view/layout.ts";
import { BUILTIN_WIDGETS, INTERACTION_WIDGETS } from "../view/registry.ts";
import { ViewRenderer } from "../view/render.tsx";
import { resolveWidget, type WidgetRegistry } from "../view/resolve.ts";
import { WidgetBoundary } from "../view/slot.tsx";
import { type ResolvedView, resolveViews, type Views } from "../view/views.ts";
import { InteractionsWidget } from "../widgets/chat/index.ts";
import { ThreadsWidget } from "../widgets/threads/index.ts";
import { AgentFiles } from "./agent-files.tsx";
import { AgentNotices } from "./agent-notices.tsx";
import { CommandLine } from "./command-line.tsx";
import { GlobalCommands } from "./commands.tsx";
import { DeleteThreadDialog, DeleteWorkspaceDialog } from "./dialogs.tsx";
import { FilePickerLine } from "./file-picker.tsx";
import { FloatingFile } from "./floating-file.tsx";
import { FloatingWidgets } from "./floating-widgets.tsx";
import { Help } from "./help.tsx";
import { AddWidget, LAYOUT_EDIT, LayoutEditToggle, useLayoutWriter } from "./layout.tsx";
import { ModeLine, useMode } from "./mode-line.tsx";
import type { NotificationPort } from "./notifications.ts";
import { Notifier } from "./notifier.tsx";
import { breakpointOf, LEFT_OPEN, MOBILE_AREA } from "./responsive.ts";
import {
  Areas,
  LEFT_COLUMN_ID,
  LeftSplitter,
  LeftStrip,
  ToggleThreadsCommand,
  useLeftDrawer,
  useVisualViewport,
} from "./responsive.tsx";
import { ConnectionBanner, Errors, StatusLiveRegion } from "./status.tsx";

export interface FrameProps {
  /** Built-in widgets by reference; {@link BUILTIN_WIDGETS} by default. */
  readonly registry?: WidgetRegistry;
  /** The defaults the daemon's `[keymap]` entries apply to; {@link DEFAULT_BINDINGS} by default. */
  readonly bindings?: readonly Binding[];
  /** Clock of key sequences; `performance.now` by default. */
  readonly now?: () => number;
  /** The Notification API; none by default (main.tsx passes the browser's). */
  readonly notifications?: NotificationPort;
  readonly hasFocus?: () => boolean;
  readonly focusWindow?: () => void;
}

const performanceNow = () => performance.now();
const documentHasFocus = () => document.hasFocus();
const focusThisWindow = () => window.focus();

export function Frame({
  registry = BUILTIN_WIDGETS,
  bindings: defaults = DEFAULT_BINDINGS,
  now = performanceNow,
  notifications,
  hasFocus = documentHasFocus,
  focusWindow = focusThisWindow,
}: FrameProps): ReactNode {
  const workspaceId = useCurrentWorkspaceId();
  const state = useViews(workspaceId);
  useWindowReports(workspaceId);
  const manifest = state?.manifest;
  const manifestError = state?.error;
  const views = useMemo(() => viewsOf(manifest, manifestError), [manifest, manifestError]);
  const [active, setActive] = useWindowState(ACTIVE_VIEW);
  const view =
    views.views.find((candidate) => candidate.id === active) ??
    views.views.find((candidate) => candidate.id === views.defaultId);
  const [layoutEdit] = useWindowState(LAYOUT_EDIT);
  const editing = layoutEdit && workspaceId !== undefined;
  const write = useLayoutWriter(workspaceId, state, view?.id ?? views.defaultId);
  const settings = useSettings();
  // The defaults < the daemon's `[keymap]` entries < the current workspace's `.omfp/keymap.json`, live (UI-097, UI-210).
  const bindings = useBindings(defaults, settings.keymap, state);
  useKeymap(bindings, now);
  useAppearance();
  useUserTheme();
  const appRef = useRef<HTMLDivElement>(null);
  const asideRef = useRef<HTMLElement>(null);
  // The frame's own width picks the layout (UI-200).
  const breakpoint = useWidthClass(appRef, breakpointOf);
  useLeftDrawer(breakpoint, asideRef);
  useVisualViewport(breakpoint);
  const [leftOpen] = useWindowState(LEFT_OPEN);
  const [area] = useWindowState(MOBILE_AREA);
  const side = view !== undefined && chatCell(view.tree) !== undefined;
  const viewIds = views.views.map((candidate) => candidate.id);
  return (
    <div ref={appRef} className="omfp-app" data-testid="app" data-layout={breakpoint}>
      <ConnectionBanner />
      {views.error !== undefined && (
        <div className="omfp-banner" data-tone="error" role="alert" data-testid="view-error">
          {strings.shell.frame.invalidView(views.error)}
        </div>
      )}
      <KeymapError views={state} />
      {settings.error !== null && settings.error !== undefined && (
        <div className="omfp-banner" data-tone="error" role="alert" data-testid="settings-error">
          {strings.shell.settings.error(settings.error)}
        </div>
      )}
      <main className="omfp-shell" data-testid="shell">
        {breakpoint === "narrow" && <LeftStrip />}
        <aside
          ref={asideRef}
          id={LEFT_COLUMN_ID}
          className="omfp-frame__left"
          data-testid="left-column"
          data-open={(breakpoint !== "wide" && leftOpen) || undefined}
        >
          <div className="omfp-frame__threads">
            <WidgetBoundary reference="omfp:threads">
              <ThreadsWidget />
            </WidgetBoundary>
          </div>
          <div className="omfp-frame__views" data-testid="view-controls">
            <Select
              label={strings.shell.frame.views}
              hideLabel
              data-testid="view-switcher"
              items={views.views.map((candidate) => ({ id: candidate.id, label: candidate.title }))}
              selectedKey={view?.id ?? null}
              onSelectionChange={(key) => {
                if (key !== null) {
                  setActive(String(key));
                }
              }}
            />
            <LayoutEditToggle editing={editing} enabled={workspaceId !== undefined} />
            {editing && (
              <AddWidget
                registry={registry}
                views={state}
                onAdd={(reference) => {
                  void write({ kind: "add", reference });
                }}
              />
            )}
          </div>
        </aside>
        {breakpoint === "wide" && <LeftSplitter aside={asideRef} />}
        <div className="omfp-frame__main">
          {view !== undefined && (
            <ViewRenderer
              view={view}
              registry={registry}
              workspaceId={workspaceId}
              views={state}
              onEdit={editing ? write : undefined}
              area={breakpoint === "mobile" ? (side ? area : "chat") : undefined}
            />
          )}
          {view !== undefined && !hasMainSlot(view.tree) && (
            <FloatingFile workspaceId={workspaceId} views={state} registry={registry} />
          )}
          <FloatingWidgets panels={views.panels} workspaceId={workspaceId} views={state} registry={registry} />
          <FrameModeLine appRef={appRef} bindings={bindings} />
        </div>
      </main>
      {breakpoint === "mobile" && <Areas side={side} />}
      <ToggleThreadsCommand breakpoint={breakpoint} />
      {view !== undefined && <SafetyNet view={view} workspaceId={workspaceId} views={state} registry={registry} />}
      <Errors />
      <StatusLiveRegion />
      <DeleteThreadDialog />
      <DeleteWorkspaceDialog />
      <Help bindings={bindings} now={now} />
      <GlobalCommands viewIds={viewIds} />
      <Notifier port={notifications} hasFocus={hasFocus} focusWindow={focusWindow} />
      <AgentFiles />
      <AgentNotices />
    </div>
  );
}

/** The mode line and the frame's `data-mode`: a mode change (focusing the composer) renders them, not the frame. */
function FrameModeLine({
  appRef,
  bindings,
}: {
  appRef: RefObject<HTMLDivElement | null>;
  bindings: readonly Binding[];
}): ReactNode {
  const mode = useMode();
  const [editorMode] = useWindowState(EDITOR_MODE);
  // After the frame's ref is set (a child's layout effect runs before it on mount).
  useEffect(() => {
    appRef.current?.setAttribute("data-mode", mode);
  }, [appRef, mode]);
  return (
    <ModeLine mode={mode} vim={mode === "insert" ? editorMode : undefined} bindings={bindings}>
      <CommandLine bindings={bindings} />
      <FilePickerLine />
    </ModeLine>
  );
}

/** The workspace this window works in, live: the selected thread's, else the draft thread's (UI-100). */
function useCurrentWorkspaceId(): string | undefined {
  const [selected] = useWindowState(SELECTED_THREAD);
  const [draft] = useWindowState(DRAFT_THREAD);
  const summary = useThreadSummary(selected);
  return selected === undefined ? draft?.workspaceId : summary?.workspaceId;
}

/** The views of the workspace's manifest; the default view without one, before it arrives, or with the daemon's
 * error (AGT-011). */
function viewsOf(manifest: unknown, error: string | null | undefined): Views {
  if (error !== undefined && error !== null) {
    return { ...resolveViews(undefined), error };
  }
  return resolveViews(manifest);
}

/**
 * The window's widget reports (AGT-026): tells the reporter which workspace it shows and the views topic position it
 * rendered, follows the window's console errors, and sends the report again on every new connection (the daemon
 * forgets a closed connection's report, PRT-043).
 */
function useWindowReports(workspaceId: string | undefined): void {
  const { reports } = useHost();
  const position = useSnapshot((snapshot) =>
    workspaceId === undefined
      ? undefined
      : snapshot.topics.find((topic) => topic.topic === viewsTopic(workspaceId))?.position,
  );
  const epoch = position?.epoch;
  const seq = position?.seq;
  useEffect(() => {
    reports.show(workspaceId, epoch === undefined || seq === undefined ? undefined : { epoch, seq });
  }, [reports, workspaceId, epoch, seq]);
  useEffect(() => reports.watchConsole(window), [reports]);
  const hello = useSnapshot((snapshot) =>
    snapshot.connection.kind === "connected" ? snapshot.connection.hello : undefined,
  );
  useEffect(() => {
    if (hello !== undefined) {
      reports.resend();
    }
  }, [reports, hello]);
}

/** One global key handler for the window (UI-095). */
function useKeymap(bindings: readonly Binding[], now: () => number): void {
  const { commands } = useHost();
  useEffect(() => {
    const handler = createKeyHandler({ bindings, commands, now });
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [bindings, commands, now]);
}

interface SafetyNetProps {
  readonly view: ResolvedView;
  readonly workspaceId: string | undefined;
  readonly views: ViewsState | undefined;
  readonly registry: WidgetRegistry;
}

/**
 * A view without the built-in `omfp:chat` or `omfp:interactions` can never hide an approval: the frame renders the
 * interactions itself (AGT-015). A workspace widget overriding them (AGT-022) does not count.
 */
function SafetyNet({ view, workspaceId, views, registry }: SafetyNetProps): ReactNode {
  const covered = INTERACTION_WIDGETS.some(
    (reference) =>
      containsWidget(view.tree, [reference]) &&
      resolveWidget(reference, workspaceId, views, registry).kind === "builtin",
  );
  if (covered) {
    return null;
  }
  return (
    <aside className="omfp-safety-net" data-testid="safety-net">
      <WidgetBoundary reference="omfp:interactions">
        <InteractionsWidget />
      </WidgetBoundary>
    </aside>
  );
}
