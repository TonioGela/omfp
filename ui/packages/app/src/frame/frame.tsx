/**
 * The thin frame around the view (docs/specs/08-agent-control.md): connection banner (UI-003), view switcher, the
 * keymap and `:` command line (UI-095), the safety net for pending interactions, notifications (UI-096), the live
 * region (UI-092), dialogs, and theme/density on `<html>` (UI-091, UI-094). Everything else is a widget of the view.
 */

import { Select } from "@omfp/kit";
import { type ReactNode, useEffect, useMemo } from "react";
import { createKeyHandler } from "../keymap/handler.ts";
import { type Binding, DEFAULT_BINDINGS, mergeBindings } from "../keymap/keymap.ts";
import { ACTIVE_VIEW, useHost, useSettings, useWindowState } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { useAppearance, useUserTheme } from "../theme/theme.ts";
import { containsWidget } from "../view/layout.ts";
import { BUILTIN_WIDGETS, INTERACTION_WIDGETS } from "../view/registry.ts";
import { ViewRenderer, WidgetBoundary, type WidgetRegistry } from "../view/render.tsx";
import { type ResolvedView, resolveViews } from "../view/views.ts";
import { InteractionsWidget } from "../widgets/chat/index.ts";
import { NewThreadDialog } from "../widgets/new-thread/index.ts";
import { CommandLine } from "./command-line.tsx";
import { GlobalCommands } from "./commands.tsx";
import { DeleteThreadDialog } from "./dialogs.tsx";
import type { NotificationPort } from "./notifications.ts";
import { Notifier } from "./notifier.tsx";
import { ConnectionBanner, Errors, StatusLiveRegion } from "./status.tsx";

export interface FrameProps {
  /** Widgets by name; the built-ins by default. */
  readonly registry?: WidgetRegistry;
  /** A `views.json`-shaped manifest; the default view when absent or invalid. */
  readonly views?: unknown;
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
  views: manifest,
  bindings: defaults = DEFAULT_BINDINGS,
  now = performanceNow,
  notifications,
  hasFocus = documentHasFocus,
  focusWindow = focusThisWindow,
}: FrameProps): ReactNode {
  const views = useMemo(() => resolveViews(manifest), [manifest]);
  const [active, setActive] = useWindowState(ACTIVE_VIEW);
  const view =
    views.views.find((candidate) => candidate.id === active) ??
    views.views.find((candidate) => candidate.id === views.defaultId);
  const settings = useSettings();
  // The daemon's `[keymap]` entries over the defaults, live (UI-097).
  const bindings = useMemo(() => mergeBindings(defaults, settings.keymap), [defaults, settings.keymap]);
  useKeymap(bindings, now);
  useAppearance();
  useUserTheme();
  const viewIds = views.views.map((candidate) => candidate.id);
  return (
    <div className="omfp-app" data-testid="app">
      <ConnectionBanner />
      {views.error !== undefined && (
        <div className="omfp-banner" data-tone="error" role="alert" data-testid="view-error">
          {strings.shell.frame.invalidView(views.error)}
        </div>
      )}
      {settings.error !== null && settings.error !== undefined && (
        <div className="omfp-banner" data-tone="error" role="alert" data-testid="settings-error">
          {strings.shell.settings.error(settings.error)}
        </div>
      )}
      <div className="omfp-frame__bar">
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
      </div>
      <main className="omfp-shell" data-testid="shell">
        {view !== undefined && <ViewRenderer view={view} registry={registry} />}
      </main>
      {view !== undefined && <SafetyNet view={view} />}
      <CommandLine bindings={bindings} />
      <Errors />
      <StatusLiveRegion />
      <NewThreadDialog />
      <DeleteThreadDialog />
      <GlobalCommands viewIds={viewIds} />
      <Notifier port={notifications} hasFocus={hasFocus} focusWindow={focusWindow} />
    </div>
  );
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

/** A view without `omfp:chat` or `omfp:interactions` can never hide an approval: the frame renders them itself. */
function SafetyNet({ view }: { view: ResolvedView }): ReactNode {
  if (containsWidget(view.tree, INTERACTION_WIDGETS)) {
    return null;
  }
  return (
    <aside className="omfp-safety-net" data-testid="safety-net">
      <WidgetBoundary name="omfp:interactions">
        <InteractionsWidget />
      </WidgetBoundary>
    </aside>
  );
}
