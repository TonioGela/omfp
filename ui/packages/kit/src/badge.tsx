import type { HTMLAttributes, ReactNode } from "react";
import { IconButton } from "./button.tsx";
import { cx } from "./class.ts";

export type Tone = "default" | "primary" | "error" | "warning" | "success";

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
  children?: ReactNode;
}

/** A small static status label (counts, states). */
export function Badge({ tone = "default", className, ...props }: BadgeProps) {
  return <span {...props} className={cx("omfp-badge", className)} data-tone={tone} />;
}

export interface ChipProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
  /** Shows a remove button that calls it. */
  onRemove?: () => void;
  /** Accessible name of the remove button. */
  removeLabel?: string;
  /** `data-testid` of the remove button (`remove` by default). */
  removeTestId?: string;
  children?: ReactNode;
}

/** A compact token (attachment, filter, queued item), optionally removable. */
export function Chip({
  tone = "default",
  onRemove,
  removeLabel = "Remove",
  removeTestId = "remove",
  className,
  children,
  ...props
}: ChipProps) {
  return (
    <span {...props} className={cx("omfp-chip", className)} data-tone={tone}>
      <span className="omfp-chip__label">{children}</span>
      {onRemove !== undefined && (
        <IconButton
          className="omfp-chip__remove"
          label={removeLabel}
          icon="×"
          onPress={onRemove}
          data-testid={removeTestId}
        />
      )}
    </span>
  );
}

export type KbdProps = HTMLAttributes<HTMLElement>;

/** A key or key sequence (`<kbd>`), e.g. in hints and the command line. */
export function Kbd({ className, ...props }: KbdProps) {
  return <kbd {...props} className={cx("omfp-kbd", className)} />;
}
