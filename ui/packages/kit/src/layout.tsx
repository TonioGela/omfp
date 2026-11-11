import {
  type ComponentProps,
  type HTMLAttributes,
  type ReactNode,
  type RefObject,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import {
  Button as AriaButton,
  Dialog as AriaDialog,
  type DialogProps as AriaDialogProps,
  Disclosure as AriaDisclosure,
  type DisclosureProps as AriaDisclosureProps,
  Popover as AriaPopover,
  type PopoverProps as AriaPopoverProps,
  Toolbar as AriaToolbar,
  type ToolbarProps as AriaToolbarProps,
  Tooltip as AriaTooltip,
  type TooltipProps as AriaTooltipProps,
  type DialogRenderProps,
  DisclosurePanel,
  Heading,
} from "react-aria-components";
import { cx, withClass } from "./class.ts";

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

/** The root font size (`rem`) in px. */
function remPx(): number {
  return Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
}

/**
 * A class of an element's width (border box, px), following it with a `ResizeObserver`: `classify` maps a width and
 * the root font size (px per `rem`) to a value (e.g. a breakpoint), and the component renders again only when that
 * value changes. Before the element is measured, or when it has no width (not laid out), the width is 0.
 */
export function useWidthClass<T>(ref: RefObject<Element | null>, classify: (width: number, rem: number) => T): T {
  const latest = useRef(classify);
  latest.current = classify;
  const [value, setValue] = useState<T>(() => classify(0, 16));
  useLayoutEffect(() => {
    const element = ref.current;
    if (element === null) {
      return;
    }
    const update = (width: number) => setValue(latest.current(width, remPx()));
    update(element.getBoundingClientRect().width);
    if (typeof ResizeObserver !== "function") {
      return;
    }
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        update(entry.borderBoxSize?.[0]?.inlineSize ?? entry.contentRect.width);
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref]);
  return value;
}

export interface PopoverDialogProps extends Omit<AriaDialogProps, "children"> {
  /** Heading on the popover's top border (`╭─ title`); it names the dialog. */
  title: ReactNode;
  placement?: AriaPopoverProps["placement"];
  /** Body; a function receives `{ close }`. */
  children: ReactNode | ((opts: DialogRenderProps) => ReactNode);
}

/**
 * A non-modal dialog in a popover, inside a `DialogTrigger` (e.g. the compact chat header's `⋯`, UI-202): the
 * popover surface with its title on the top border, focus moved in, Escape and a click outside close it.
 */
export function PopoverDialog({ title, placement = "bottom end", className, children, ...props }: PopoverDialogProps) {
  return (
    <AriaPopover placement={placement} className="omfp-popover omfp-popdialog">
      <AriaDialog {...props} className={cx("omfp-popdialog__dialog", className)}>
        {(renderProps) => (
          <>
            <Heading slot="title" className="omfp-popdialog__title">
              {title}
            </Heading>
            {typeof children === "function" ? children(renderProps) : children}
          </>
        )}
      </AriaDialog>
    </AriaPopover>
  );
}

export interface SplitterProps extends Omit<HTMLAttributes<HTMLDivElement>, "role" | "children"> {
  /** `vertical`: a bar between cells side by side (it moves left and right); `horizontal`: between stacked cells. */
  orientation: "vertical" | "horizontal";
  /** Accessible name, e.g. what it resizes. */
  label: string;
  /** Share of the cell before it, 0–100. */
  value: number;
}

/**
 * A focusable window splitter (WAI-ARIA `separator`, UI-206): a 1 px hairline in `rule` inside a wider grab
 * area that fills with the gradient on hover, focus and while dragged (`data-dragging`). The app wires the pointer,
 * the arrow keys and double click.
 */
export function Splitter({ orientation, label, value, className, ...props }: SplitterProps) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a focusable, adjustable separator (WAI-ARIA window splitter); <hr> cannot take focus or keys.
    <div
      tabIndex={0}
      {...props}
      role="separator"
      aria-orientation={orientation}
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value)}
      className={cx("omfp-splitter", className)}
    />
  );
}
