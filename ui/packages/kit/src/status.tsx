import { type HTMLAttributes, useLayoutEffect, useRef } from "react";
import {
  ProgressBar as AriaProgressBar,
  type ProgressBarProps as AriaProgressBarProps,
  Label,
} from "react-aria-components";
import { cx, withClass } from "./class.ts";

export interface ProgressProps
  extends Omit<AriaProgressBarProps, "children" | "value" | "maxValue" | "isIndeterminate" | "aria-label"> {
  /** Current value; omit for an indeterminate (busy) bar. */
  value?: number;
  /** Upper bound (100 by default). */
  max?: number;
  /** Names the bar (visible unless `hideLabel`). */
  label: string;
  hideLabel?: boolean;
}

/** A progress bar, indeterminate when it has no `value`. */
export function Progress({ value, max = 100, label, hideLabel = false, className, ...props }: ProgressProps) {
  const indeterminate = value === undefined;
  return (
    <AriaProgressBar
      {...props}
      {...(indeterminate ? { isIndeterminate: true } : { value, maxValue: max })}
      className={withClass("omfp-progress", className)}
    >
      {({ percentage, valueText }) => (
        <>
          <div className={cx("omfp-progress__head", hideLabel && "omfp-visually-hidden")}>
            <Label className="omfp-progress__label">{label}</Label>
            {!indeterminate && <span className="omfp-progress__value">{valueText}</span>}
          </div>
          <div className="omfp-progress__track">
            <div
              className="omfp-progress__fill"
              style={indeterminate ? undefined : { inlineSize: `${percentage ?? 0}%` }}
            />
          </div>
        </>
      )}
    </AriaProgressBar>
  );
}

/** Distance in pixels from the end under which the log counts as following it. */
const FOLLOW_SLACK = 4;

export interface LogProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  lines: readonly string[];
}

/**
 * Monospace output (`role="log"`, focusable so it scrolls from the keyboard). New lines keep it scrolled to the end
 * unless the user scrolled up.
 */
export function Log({ lines, className, onScroll, ...props }: LogProps) {
  const ref = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  // biome-ignore lint/correctness/useExhaustiveDependencies: scrolls again whenever the lines change
  useLayoutEffect(() => {
    const element = ref.current;
    if (element !== null && following.current) {
      element.scrollTop = element.scrollHeight;
    }
  }, [lines]);
  return (
    <div
      // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable log must be keyboard reachable (UI-092)
      tabIndex={0}
      role="log"
      {...props}
      ref={ref}
      className={cx("omfp-log", className)}
      onScroll={(event) => {
        const element = event.currentTarget;
        following.current = element.scrollHeight - element.scrollTop - element.clientHeight <= FOLLOW_SLACK;
        onScroll?.(event);
      }}
    >
      {lines.map((line, index) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: log lines are append-only, the index is their identity
        <div key={index} className="omfp-log__line">
          {line}
        </div>
      ))}
    </div>
  );
}

export interface LiveRegionProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  /** The latest status message; screen readers announce each change politely. */
  message: string;
}

/** A visually hidden polite live region (`role="status"`) announcing status changes (UI-092). */
export function LiveRegion({ message, className, ...props }: LiveRegionProps) {
  return (
    <div
      role="status"
      aria-live="polite"
      aria-atomic="true"
      {...props}
      className={cx("omfp-visually-hidden", className)}
    >
      {message}
    </div>
  );
}
