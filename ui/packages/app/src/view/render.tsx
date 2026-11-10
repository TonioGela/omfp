/**
 * Renders a validated view (UI-024): containers become flex rows and columns, tabs or stacked layers; each widget
 * renders from the registry inside its own error boundary, so a failing widget shows its error in place and the rest
 * of the window keeps working.
 */

import { Tab, TabList, TabPanel, Tabs } from "@omfp/kit";
import { Component, type ReactNode } from "react";
import type { WidgetComponent, WidgetProps } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import type { LayoutTree } from "./layout.ts";
import type { ResolvedView } from "./views.ts";

export type WidgetRegistry = ReadonlyMap<string, WidgetComponent>;

const text = strings.shell.frame;

export function ViewRenderer({ view, registry }: { view: ResolvedView; registry: WidgetRegistry }): ReactNode {
  return (
    <div className="omfp-view" data-testid="view" data-view={view.id}>
      <Node tree={view.tree} registry={registry} path="0" />
    </div>
  );
}

function Node({ tree, registry, path }: { tree: LayoutTree; registry: WidgetRegistry; path: string }): ReactNode {
  if (tree.kind === "widget") {
    return <WidgetSlot name={tree.name} props={tree.props} registry={registry} />;
  }
  const children = tree.children.map((child, index) => ({ child, path: `${path}.${index}` }));
  if (tree.kind === "tabs") {
    return (
      <Tabs className="omfp-view__tabs" data-layout="tabs">
        <TabList aria-label={text.views}>
          {children.map(({ child, path: id }) => (
            <Tab key={id} id={id} data-testid="view-tab">
              {nodeTitle(child)}
            </Tab>
          ))}
        </TabList>
        {children.map(({ child, path: id }) => (
          <TabPanel key={id} id={id} className="omfp-view__panel">
            <Node tree={child} registry={registry} path={id} />
          </TabPanel>
        ))}
      </Tabs>
    );
  }
  const { sizes } = tree.props;
  return (
    <div className={`omfp-view__${tree.kind}`} data-layout={tree.kind}>
      {children.map(({ child, path: id }, index) => (
        <div key={id} className="omfp-view__cell" style={sizes === undefined ? undefined : { flexGrow: sizes[index] }}>
          <Node tree={child} registry={registry} path={id} />
        </div>
      ))}
    </div>
  );
}

/** A tab's label: the widget's `title` prop, else its name; a container's kind. */
function nodeTitle(tree: LayoutTree): string {
  if (tree.kind !== "widget") {
    return tree.kind;
  }
  return typeof tree.props.title === "string" ? tree.props.title : tree.name;
}

function WidgetSlot({ name, props, registry }: { name: string; props: WidgetProps; registry: WidgetRegistry }) {
  const Widget = registry.get(name);
  return (
    <div className="omfp-widget" data-testid="widget" data-widget={name}>
      <WidgetBoundary name={name}>
        {Widget === undefined ? (
          <p className="omfp-widget__error" role="alert" data-testid="widget-error">
            {text.unknownWidget(name)}
          </p>
        ) : (
          <Widget {...props} />
        )}
      </WidgetBoundary>
    </div>
  );
}

interface BoundaryProps {
  readonly name: string;
  readonly children: ReactNode;
}

interface BoundaryState {
  readonly error: Error | undefined;
}

/** Catches a widget's render errors and shows them in its place. */
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
    return (
      <div className="omfp-widget__error" role="alert" data-testid="widget-error">
        <strong>{text.widgetFailed(this.props.name)}</strong>
        <pre>{error.message}</pre>
      </div>
    );
  }
}
