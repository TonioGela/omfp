import type { ComponentProps, ReactNode } from "react";
import {
  Button as AriaButton,
  Disclosure as AriaDisclosure,
  type DisclosureProps as AriaDisclosureProps,
  Toolbar as AriaToolbar,
  type ToolbarProps as AriaToolbarProps,
  Tooltip as AriaTooltip,
  type TooltipProps as AriaTooltipProps,
  DisclosurePanel,
  Heading,
} from "react-aria-components";
import { withClass } from "./class.ts";

export { TooltipTrigger } from "react-aria-components";

export type ToolbarProps = AriaToolbarProps;

/** A group of controls reached with one Tab stop; arrow keys move inside. */
export function Toolbar({ className, ...props }: ToolbarProps) {
  return <AriaToolbar {...props} className={withClass("omfp-toolbar", className)} />;
}

export type TooltipProps = AriaTooltipProps;

/** A hint shown on hover/focus of the trigger inside a `TooltipTrigger`. */
export function Tooltip({ className, offset = 4, ...props }: TooltipProps) {
  return <AriaTooltip {...props} offset={offset} className={withClass("omfp-tooltip", className)} />;
}

export interface DisclosureProps
  extends Omit<AriaDisclosureProps, "children" | "isExpanded" | "onExpandedChange" | "defaultExpanded"> {
  /** Text of the toggle button. */
  title: ReactNode;
  /** Controlled state; omit both for an uncontrolled disclosure that starts collapsed. */
  expanded?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  /** Heading level of the toggle (3 by default). */
  level?: number;
  /** `data-testid` of the toggle button (`toggle` by default). */
  toggleTestId?: string;
  children?: ReactNode;
}

/** A section that expands and collapses under a toggle button (e.g. thinking blocks, raw JSON). */
export function Disclosure({
  title,
  expanded,
  onExpandedChange,
  level = 3,
  className,
  toggleTestId = "toggle",
  id,
  children,
  ...props
}: DisclosureProps) {
  return (
    <AriaDisclosure
      {...props}
      {...(expanded === undefined ? {} : { isExpanded: expanded })}
      {...(onExpandedChange === undefined ? {} : { onExpandedChange })}
      {...(id === undefined ? {} : { id, render: (dom: ComponentProps<"div">) => <div {...dom} id={String(id)} /> })}
      className={withClass("omfp-disclosure", className)}
    >
      <Heading level={level} className="omfp-disclosure__heading">
        <AriaButton slot="trigger" className="omfp-disclosure__trigger" data-testid={toggleTestId}>
          <span className="omfp-disclosure__chevron" aria-hidden="true" />
          {title}
        </AriaButton>
      </Heading>
      <DisclosurePanel className="omfp-disclosure__panel">{children}</DisclosurePanel>
    </AriaDisclosure>
  );
}
