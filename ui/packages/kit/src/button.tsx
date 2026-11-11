import type { ReactNode } from "react";
import { Button as AriaButton, type ButtonProps as AriaButtonProps } from "react-aria-components";
import { withClass } from "./class.ts";
import { Icon } from "./icons.tsx";
import { Kbd } from "./kbd.tsx";

export type ButtonVariant = "primary" | "secondary" | "quiet" | "danger";
export type ButtonSize = "sm" | "md";

export interface ButtonProps extends AriaButtonProps {
  /** Visual weight; `secondary` by default. */
  variant?: ButtonVariant;
  /** `md` by default; `sm` for dense rows. */
  size?: ButtonSize;
  /** A leading icon (an `Icon`); while `isPending` it is replaced by a spinning one. */
  icon?: ReactNode;
  /** A trailing key chip (UI-148), e.g. `enter`. */
  kbd?: string | undefined;
}

/**
 * A push button (UI-149; React Aria `Button`: press events, keyboard, focus ring via `data-focus-visible`).
 * `isPending` makes it busy: a spinning icon, the label kept, presses ignored.
 */
export function Button({
  variant = "secondary",
  size = "md",
  icon,
  kbd,
  isPending,
  className,
  children,
  ...props
}: ButtonProps) {
  const decorated = icon !== undefined || kbd !== undefined || isPending === true;
  return (
    <AriaButton
      {...props}
      {...(isPending === undefined ? {} : { isPending })}
      className={withClass("omfp-button", className)}
      data-variant={variant}
      data-size={size}
    >
      {decorated && typeof children !== "function" ? (
        <>
          {isPending === true ? <Icon name="loader-circle" className="omfp-button__spinner" /> : icon}
          {children}
          {kbd !== undefined && <Kbd aria-hidden="true">{kbd}</Kbd>}
        </>
      ) : (
        children
      )}
    </AriaButton>
  );
}

export interface IconButtonProps extends Omit<ButtonProps, "children" | "aria-label" | "icon" | "kbd"> {
  /** Accessible name (becomes `aria-label`); icon-only buttons have no visible text. */
  label: string;
  /** The icon (an `Icon`) or glyph shown; it is hidden from assistive technology. */
  icon: ReactNode;
}

/** A square button that shows only an icon (UI-149); `label` names it for screen readers. */
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
