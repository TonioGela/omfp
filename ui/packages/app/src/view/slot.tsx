/**
 * The slot every widget of a view renders in (UI-105, AGT-013, AGT-014): it resolves the reference (built-in,
 * workspace override, workspace or user widget), imports module widgets from the daemon by versioned URL, provides the
 * {@link WidgetScope} the SDK's widget hooks read, and shows loading and every failure in the widget's place.
 */

import type { ViewsState, WidgetInfo } from "@omfp/protocol";
import { Component, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { type LoadedWidget, useHost, useSnapshot, type WidgetProps, WidgetScope } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { resolveWidget, type WidgetRegistry, widgetUrl } from "./resolve.ts";

const text = strings.widgets;

export interface WidgetSlotProps {
  /** The node's widget reference (`omfp:chat`, `./widgets/tests`, `user:clock`). */
  readonly reference: string;
  readonly props: WidgetProps;
  /** The node's path in the view (`"0.1"`, UI-103). */
  readonly path: string;
  /** The workspace whose view this is; `undefined` without a current workspace. */
  readonly workspaceId: string | undefined;
  /** That workspace's views state (`useViews`), `undefined` until it arrives. */
  readonly views: ViewsState | undefined;
  readonly registry: WidgetRegistry;
}

/** A module as shown: the folder it came from, its version and what its import gave. */
interface Shown {
  readonly folder: string;
  readonly version: string;
  readonly loaded: LoadedWidget;
}

const NO_ENTRY: LoadedWidget = {
  kind: "error",
  error: { kind: "load", message: text.noEntry, file: undefined, line: null, column: null },
};

/** The `widgetBase` of the latest `system.hello` (PRT-042), kept while reconnecting. */
function useWidgetBase(): string | undefined {
  const current = useSnapshot((snapshot) =>
    snapshot.connection.kind === "connected" ? snapshot.connection.hello.widgetBase : undefined,
  );
  const latest = useRef(current);
  if (current !== undefined) {
    latest.current = current;
  }
  return latest.current;
}

/**
 * The module of `widget`: imported once per URL; while a new version loads, the previous one of the same folder stays
 * shown, so a reload replaces the widget without a placeholder flash (AGT-013). `undefined` while nothing is loaded.
 */
function useWidgetModule(widget: WidgetInfo | undefined, workspaceId: string | undefined): Shown | undefined {
  const { modules } = useHost();
  const base = useWidgetBase();
  const folder = widget === undefined ? undefined : `${widget.source}/${widget.name}`;
  const version = widget?.version;
  const entry = widget?.entry ?? undefined;
  const url =
    widget === undefined || entry === undefined || base === undefined || workspaceId === undefined
      ? undefined
      : widgetUrl(base, workspaceId, widget, entry);
  const [shown, setShown] = useState<Shown | undefined>(undefined);
  useEffect(() => {
    if (url === undefined || folder === undefined || version === undefined || modules.get(url) !== undefined) {
      return;
    }
    let live = true;
    void modules.load(url).then((loaded) => {
      if (live) {
        setShown({ folder, version, loaded });
      }
    });
    return () => {
      live = false;
    };
  }, [modules, url, folder, version]);

  if (folder === undefined || version === undefined) {
    return undefined;
  }
  if (widget?.entry === null || widget?.entry === undefined) {
    return { folder, version, loaded: NO_ENTRY };
  }
  const settled = url === undefined ? undefined : modules.get(url);
  if (settled !== undefined) {
    return { folder, version, loaded: settled };
  }
  return shown?.folder === folder ? shown : undefined;
}

export function WidgetSlot({ reference, props, path, workspaceId, views, registry }: WidgetSlotProps): ReactNode {
  const instance = typeof props.instance === "string" ? props.instance : reference;
  const scope = useMemo(() => ({ instance, reference, workspaceId }), [instance, reference, workspaceId]);
  const resolved = resolveWidget(reference, workspaceId, views, registry);
  const module = useWidgetModule(resolved.kind === "module" ? resolved.widget : undefined, workspaceId);

  let body: ReactNode;
  switch (resolved.kind) {
    case "builtin": {
      const Widget = resolved.component;
      body = (
        <WidgetBoundary key={reference} reference={reference}>
          <Widget {...props} />
        </WidgetBoundary>
      );
      break;
    }
    case "missing":
      body = <WidgetError kind="missing" title={text.missing(reference)} />;
      break;
    case "pending":
      body = <WidgetLoading reference={reference} />;
      break;
    case "module":
      body = moduleBody(reference, props, module);
      break;
  }
  const moduleAttributes =
    resolved.kind === "module"
      ? { "data-source": resolved.widget.source, ...(module === undefined ? {} : { "data-version": module.version }) }
      : {};
  return (
    <div
      className="omfp-widget"
      data-testid="widget"
      data-widget={reference}
      data-instance={instance}
      data-path={path}
      {...moduleAttributes}
    >
      <WidgetScope value={scope}>{body}</WidgetScope>
    </div>
  );
}

function moduleBody(reference: string, props: WidgetProps, module: Shown | undefined): ReactNode {
  if (module === undefined) {
    return <WidgetLoading reference={reference} />;
  }
  const { loaded } = module;
  if (loaded.kind === "error") {
    const { error } = loaded;
    return (
      <WidgetError
        kind={error.kind}
        title={error.kind === "compile" ? text.compileFailed(reference) : text.loadFailed(reference)}
        location={error.file === undefined ? undefined : text.location(error.file, error.line, error.column)}
        message={error.message}
      />
    );
  }
  const Widget = loaded.component;
  // A new version is a new boundary: a render error of the previous one does not stick (AGT-014).
  return (
    <WidgetBoundary key={module.version} reference={reference}>
      <Widget {...props} />
    </WidgetBoundary>
  );
}

function WidgetLoading({ reference }: { reference: string }): ReactNode {
  return (
    <p className="omfp-widget__loading" data-testid="widget-loading" aria-busy="true">
      {text.loading(reference)}
    </p>
  );
}

export type WidgetErrorKind = "compile" | "load" | "render" | "missing";

interface WidgetErrorProps {
  readonly kind: WidgetErrorKind;
  readonly title: string;
  /** `file:line:column` when known. */
  readonly location?: string | undefined;
  readonly message?: string | undefined;
}

/** A widget's failure, in its place (AGT-014). */
export function WidgetError({ kind, title, location, message }: WidgetErrorProps): ReactNode {
  return (
    <div className="omfp-widget__error" role="alert" data-testid="widget-error" data-error-kind={kind}>
      <strong>{title}</strong>
      {location === undefined ? null : <code data-testid="widget-error-location">{location}</code>}
      {message === undefined ? null : <pre>{message}</pre>}
    </div>
  );
}

interface BoundaryProps {
  /** The widget's reference, named in the error. */
  readonly reference: string;
  readonly children: ReactNode;
}

interface BoundaryState {
  readonly error: Error | undefined;
}

/** Catches a widget's render errors and shows them in its place (`data-error-kind="render"`). */
export class WidgetBoundary extends Component<BoundaryProps, BoundaryState> {
  override state: BoundaryState = { error: undefined };

  static getDerivedStateFromError(error: unknown): BoundaryState {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  override render(): ReactNode {
    const { error } = this.state;
    if (error === undefined) {
      return this.props.children;
    }
    return <WidgetError kind="render" title={text.renderFailed(this.props.reference)} message={error.message} />;
  }
}
