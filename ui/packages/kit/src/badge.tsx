import type { HTMLAttributes, ReactNode } from "react";
import { IconButton } from "./button.tsx";
import { cx } from "./class.ts";
import { Icon } from "./icons.tsx";

export type Tone = "default" | "primary" | "error" | "warning" | "success";

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
  children?: ReactNode;
}

/** A small static label (UI-157): transparent, bordered in its tone's color. */
export function Badge({ tone = "default", className, ...props }: BadgeProps) {
  return <span {...props} className={cx("omfp-badge", className)} data-tone={tone} />;
}

export interface ChipProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: Tone;
  /** A leading icon (an `Icon`), drawn in primary. */
  icon?: ReactNode;
  /** `ghost`: a dashed "add" placeholder (UI-157). */
  variant?: "solid" | "ghost";
  /** Shows a remove button that calls it. */
  onRemove?: () => void;
  /** Accessible name of the remove button. */
  removeLabel?: string;
  /** `data-testid` of the remove button (`remove` by default). */
  removeTestId?: string;
  children?: ReactNode;
}

/** A compact token (attachment, workspace repo, filter, queued item), optionally removable (UI-157). */
export function Chip({
  tone = "default",
  icon,
  variant = "solid",
  onRemove,
  removeLabel = "Remove",
  removeTestId = "remove",
  className,
  children,
  ...props
}: ChipProps) {
  return (
    <span
      {...props}
      className={cx("omfp-chip", className)}
      data-tone={tone}
      data-variant={variant === "ghost" ? variant : undefined}
    >
      {icon}
      <span className="omfp-chip__label">{children}</span>
      {onRemove !== undefined && (
        <IconButton
          className="omfp-chip__remove"
          label={removeLabel}
          icon={<Icon name="x" />}
          onPress={onRemove}
          data-testid={removeTestId}
        />
      )}
    </span>
  );
}
