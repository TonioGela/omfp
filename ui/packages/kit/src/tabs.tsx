import {
  Tab as AriaTab,
  TabList as AriaTabList,
  type TabListProps as AriaTabListProps,
  TabPanel as AriaTabPanel,
  type TabPanelProps as AriaTabPanelProps,
  type TabProps as AriaTabProps,
  Tabs as AriaTabs,
  type TabsProps as AriaTabsProps,
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

export type TabPanelProps = AriaTabPanelProps;

export function TabPanel({ className, ...props }: TabPanelProps) {
  return <AriaTabPanel {...props} className={withClass("omfp-tabs__panel", className)} />;
}
