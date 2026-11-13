import { Activity, useContext } from "react";
import {
  Tab as AriaTab,
  TabList as AriaTabList,
  type TabListProps as AriaTabListProps,
  TabPanel as AriaTabPanel,
  type TabPanelProps as AriaTabPanelProps,
  type TabProps as AriaTabProps,
  Tabs as AriaTabs,
  type TabsProps as AriaTabsProps,
  TabListStateContext,
} from "react-aria-components";
import { withClass } from "./class.ts";

export type TabsProps = AriaTabsProps;

/** Tabbed panels: arrow keys move between tabs, Tab moves into the panel. */
export function Tabs({ className, ...props }: TabsProps) {
  return <AriaTabs {...props} className={withClass("omfp-tabs", className)} />;
}

export type TabListProps<T extends object> = AriaTabListProps<T>;

export function TabList<T extends object>({ className, ...props }: TabListProps<T>) {
  return <AriaTabList {...props} className={withClass("omfp-tabs__list", className)} />;
}

export type TabProps = AriaTabProps;

export function Tab({ className, ...props }: TabProps) {
  return <AriaTab {...props} className={withClass("omfp-tabs__tab", className)} />;
}

export interface TabPanelProps extends AriaTabPanelProps {
  /**
   * Keeps the panel mounted while another tab is selected, or while `hidden` (UI-402): then `hidden` and inert (out of
   * the Tab order and the accessibility tree), its content in React's `<Activity mode="hidden">`, which keeps its
   * state and DOM and cleans up its effects until it shows again. Without it the panel unmounts, as React Aria's.
   */
  readonly keepMounted?: boolean | undefined;
}

export function TabPanel({ className, keepMounted = false, hidden, children, ...props }: TabPanelProps) {
  const state = useContext(TabListStateContext);
  if (!keepMounted) {
    return (
      <AriaTabPanel {...props} hidden={hidden} className={withClass("omfp-tabs__panel", className)}>
        {children}
      </AriaTabPanel>
    );
  }
  const shown = state?.selectedKey === props.id && hidden !== true;
  return (
    <AriaTabPanel {...props} shouldForceMount hidden={!shown} className={withClass("omfp-tabs__panel", className)}>
      {(values) => (
        <Activity mode={shown ? "visible" : "hidden"}>
          {typeof children === "function" ? children(values) : children}
        </Activity>
      )}
    </AriaTabPanel>
  );
}
