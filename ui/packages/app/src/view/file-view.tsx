/**
 * TSX views in the window (UI-268): the view file `./views/<name>.tsx` of the current
 * workspace is imported from the daemon (`<widgetBase>/views/<workspaceId>/<viewFiles>/<name>.tsx`, DMN-186, a new
 * version on every save of `.omfp/views/`), its default export rendered out of sight with the SDK's layout components
 * reporting the layout they stand for, and that layout rendered like a manifest view. A compile error, a failed
 * import, a throw while rendering or an invalid layout is the view's error: the frame shows it in a banner naming the
 * file, with the default view in its place.
 */

import type { ViewsState } from "@omfp/protocol";
import { Component, type ComponentType, type ReactNode, useEffect, useMemo, useState } from "react";
import { type LoadedWidget, useHost } from "../sdk/index.ts";
import { type LayoutCollector, LayoutCollectorContext } from "../sdk/layout.tsx";
import { strings } from "../strings/index.ts";
import { type LayoutNode, type Parsed, parseLayout } from "./layout.ts";
import { useWidgetBase } from "./slot.tsx";
import type { FileView, ResolvedView } from "./views.ts";

const text = strings.init.viewFile;
const widgetText = strings.widgets;

/** The URL of a view file (DMN-186): `<widgetBase>/views/<workspaceId>/<version>/<path under .omfp/views/>`. */
export function viewFileUrl(widgetBase: string, workspaceId: string, version: string, file: string): string {
  const path = file.replace(/^\.\/views\//, "");
  return `${widgetBase}/views/${encodeURIComponent(workspaceId)}/${encodeURIComponent(version)}/${path}`;
}

/** What a view file gives the frame. */
export interface FileViewState {
  /** The rendered view; `undefined` while loading or on error. */
  readonly view: ResolvedView | undefined;
  /** Why it fails (the banner's message). */
  readonly error: string | undefined;
  /** The view component rendered out of sight; the frame mounts it. */
  readonly source: ReactNode;
}

const IDLE: FileViewState = { view: undefined, error: undefined, source: null };

/** A module's failure as the banner says it: `ops.tsx:3:5: message`. */
function loadMessage(loaded: Extract<LoadedWidget, { kind: "error" }>): string {
  const { file, line, column, message } = loaded.error;
  return file === undefined ? message : `${widgetText.location(file, line, column)}: ${message}`;
}

/** The layout a view file rendered: `key` names the workspace and file. */
interface Rendered {
  readonly key: string;
  readonly layout: Parsed<LayoutNode>;
}

/** A view file's collector: the roots' reports, and the failure of the render itself. */
interface ViewCollector extends LayoutCollector {
  /** Called after every commit of the view component. */
  settle(): void;
  fail(message: string): void;
}

/**
 * The view `listed` of `workspaceId` when it is a TSX file: imported once per version, rendered by
 * {@link FileViewState.source}. While a new version loads, the previous one stays shown.
 */
export function useFileView(
  listed: FileView | undefined,
  workspaceId: string | undefined,
  views: ViewsState | undefined,
): FileViewState {
  const { modules } = useHost();
  const base = useWidgetBase();
  const version = views?.viewFiles ?? undefined;
  const key = listed === undefined || workspaceId === undefined ? undefined : `${workspaceId}\n${listed.file}`;
  const url =
    listed === undefined || base === undefined || workspaceId === undefined || version === undefined
      ? undefined
      : viewFileUrl(base, workspaceId, version, listed.file);
  const [loaded, setLoaded] = useState<{ readonly key: string; readonly url: string; readonly loaded: LoadedWidget }>();
  useEffect(() => {
    if (url === undefined || key === undefined || modules.get(url) !== undefined) {
      return;
    }
    let live = true;
    void modules.load(url).then((module) => {
      if (live) {
        setLoaded({ key, url, loaded: module });
      }
    });
    return () => {
      live = false;
    };
  }, [modules, url, key]);
  const [rendered, setRendered] = useState<Rendered>();
  const settled = url === undefined ? undefined : modules.get(url);
  const current =
    settled !== undefined && url !== undefined ? { url, loaded: settled } : loaded?.key === key ? loaded : undefined;
  const currentUrl = current?.url;
  const collector = useMemo(
    () => (key === undefined || currentUrl === undefined ? undefined : newCollector(key, setRendered)),
    [key, currentUrl],
  );
  const shown = rendered !== undefined && rendered.key === key ? rendered.layout : undefined;
  // One view object per rendered layout: the frame keeps it (UI-390) and reports it.
  const parsed = useMemo((): Parsed<ResolvedView> | undefined => {
    if (listed === undefined || shown === undefined) {
      return undefined;
    }
    if (!shown.ok) {
      return shown;
    }
    const tree = parseLayout(shown.value, listed.file);
    if (!tree.ok) {
      return tree;
    }
    return {
      ok: true,
      value: {
        id: listed.id,
        title: listed.title,
        layout: shown.value,
        tree: tree.value,
        file: listed.file,
        ...(listed.init === undefined ? {} : { init: listed.init }),
      },
    };
  }, [listed, shown]);
  if (listed === undefined || current === undefined || collector === undefined) {
    return IDLE;
  }
  if (current.loaded.kind === "error") {
    return { view: undefined, error: loadMessage(current.loaded), source: null };
  }
  const source = (
    <ViewSource key={current.url} component={current.loaded.component as ComponentType} collector={collector} />
  );
  if (parsed === undefined) {
    return { view: undefined, error: undefined, source };
  }
  return parsed.ok
    ? { view: parsed.value, error: undefined, source }
    : { view: undefined, error: parsed.error, source };
}

/**
 * Collects the roots a view file's render reports (one each) and settles them after every commit: exactly one root is
 * the layout, none or several are errors, a throw while rendering wins. An equal result is not set again.
 */
function newCollector(
  key: string,
  set: (update: (previous: Rendered | undefined) => Rendered | undefined) => void,
): ViewCollector {
  const roots = new Map<string, Parsed<LayoutNode>>();
  let failure: string | undefined;
  const settle = () => {
    const [only, ...more] = roots.values();
    const layout: Parsed<LayoutNode> =
      failure !== undefined
        ? { ok: false, error: failure }
        : only === undefined
          ? { ok: false, error: text.noLayout }
          : more.length > 0
            ? { ok: false, error: text.manyLayouts }
            : only;
    set((previous) =>
      previous !== undefined && previous.key === key && JSON.stringify(previous.layout) === JSON.stringify(layout)
        ? previous
        : { key, layout },
    );
  };
  return {
    set: (id, layout) => {
      if (layout === undefined) {
        roots.delete(id);
      } else {
        roots.set(id, layout);
      }
    },
    settle,
    fail: (message) => {
      failure = message;
      settle();
    },
  };
}

interface ViewSourceProps {
  readonly component: ComponentType;
  readonly collector: ViewCollector;
}

/** Renders the view component; its layout roots report to `collector`, settled after their effects ran. */
function ViewSource({ component: View, collector }: ViewSourceProps): ReactNode {
  // A parent's effects run after its children's: every root of this commit has reported.
  useEffect(() => collector.settle());
  return (
    <LayoutCollectorContext.Provider value={collector}>
      <div hidden data-testid="view-source">
        <ViewBoundary onError={collector.fail}>
          <View />
        </ViewBoundary>
      </div>
    </LayoutCollectorContext.Provider>
  );
}

/** Turns a throw of the view component into the view's error. */
class ViewBoundary extends Component<{ children: ReactNode; onError: (message: string) => void }, { failed: boolean }> {
  override state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  override componentDidCatch(error: unknown): void {
    this.props.onError(error instanceof Error ? error.message : String(error));
  }

  override render(): ReactNode {
    return this.state.failed ? null : this.props.children;
  }
}
