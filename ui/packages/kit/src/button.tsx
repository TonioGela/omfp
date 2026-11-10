import type { ReactNode } from "react";
import { Button as AriaButton, type ButtonProps as AriaButtonProps } from "react-aria-components";
import { withClass } from "./class.ts";

export type ButtonVariant = "primary" | "secondary" | "quiet" | "danger";
export type ButtonSize = "sm" | "md";

export interface ButtonProps extends AriaButtonProps {
  /** Visual weight; `secondary` by default. */
  variant?: ButtonVariant;
  /** `md` by default; `sm` for dense rows. */
  size?: ButtonSize;
}

/** A push button (React Aria `Button`: press events, keyboard, focus ring via `data-focus-visible`). */
export function Button({ variant = "secondary", size = "md", className, ...props }: ButtonProps) {
  return (
    <AriaButton {...props} className={withClass("omfp-button", className)} data-variant={variant} data-size={size} />
  );
}

export interface IconButtonProps extends Omit<ButtonProps, "children" | "aria-label"> {
  /** Accessible name (becomes `aria-label`); icon-only buttons have no visible text. */
  label: string;
  /** The glyph or SVG shown; it is hidden from assistive technology. */
  icon: ReactNode;
}

/** A square button that shows only an icon; `label` names it for screen readers. */
export function IconButton({ label, icon, variant = "quiet", size = "sm", className, ...props }: IconButtonProps) {
  return (
    <AriaButton
      {...props}
      aria-label={label}
      className={withClass("omfp-button omfp-icon-button", className)}
      data-variant={variant}
      data-size={size}
    >
      <span className="omfp-icon" aria-hidden="true">
        {icon}
      </span>
    </AriaButton>
  );
}
