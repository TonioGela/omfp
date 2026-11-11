/** Toasts (UI-160) and empty states (UI-161). */
import { type HTMLAttributes, type ReactNode, useEffect, useRef } from "react";
import { IconButton } from "./button.tsx";
import { cx } from "./class.ts";
import { Icon, type IconName } from "./icons.tsx";

export type ToastTone = "info" | "success" | "error";

const TOAST_ICON: Record<ToastTone, IconName> = { info: "info", success: "circle-check", error: "circle-alert" };

/** How long info and success toasts stay (UI-160). */
export const TOAST_MS = 5000;

export interface ToastProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  tone?: ToastTone;
  children: ReactNode;
  /** A button after the text (e.g. "Retry"). */
  action?: ReactNode;
  /** Shows the quiet `×` and is called when the toast goes (closed, or timed out). */
  onDismiss?: () => void;
  /** Accessible name of the `×`. */
  dismissLabel?: string;
  /** Milliseconds before an info or success toast dismisses itself ({@link TOAST_MS}); errors stay until closed. */
  duration?: number;
}

/**
 * A notification card (UI-160): a leading icon in its tone's color, the text, an optional action and `×`. Info and
 * success are polite status messages that dismiss themselves; errors are alerts that stay.
 */
export function Toast({
  tone = "info",
  children,
  action,
  onDismiss,
  dismissLabel = "Dismiss",
  duration = TOAST_MS,
  className,
  ...props
}: ToastProps) {
  const dismiss = useRef(onDismiss);
  dismiss.current = onDismiss;
  useEffect(() => {
    if (tone === "error" || dismiss.current === undefined) {
      return undefined;
    }
    const timer = setTimeout(() => dismiss.current?.(), duration);
    return () => clearTimeout(timer);
  }, [tone, duration]);
  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      {...props}
      className={cx("omfp-toast", className)}
      data-tone={tone}
    >
      <Icon name={TOAST_ICON[tone]} className="omfp-toast__icon" />
      <span className="omfp-toast__text">{children}</span>
      {action}
      {onDismiss !== undefined && (
        <IconButton label={dismissLabel} icon={<Icon name="x" />} onPress={onDismiss} data-testid="toast-dismiss" />
      )}
    </div>
  );
}

export type ToastRegionProps = HTMLAttributes<HTMLDivElement>;

/** The bottom-right stack of toasts, newest at the bottom (UI-160). */
export function ToastRegion({ className, ...props }: ToastRegionProps) {
  return <div {...props} className={cx("omfp-toasts", className)} />;
}

export interface EmptyStateProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  icon: IconName;
  title: ReactNode;
  /** The hint, usually naming the key or command that fills the place (with key chips). */
  children?: ReactNode;
  /** A button under the hint. */
  action?: ReactNode;
  /** Without the dashed box: the empty chat (UI-161). */
  bare?: boolean;
}

/** What shows where nothing is yet (UI-161): icon, title, hint, optional action. */
export function EmptyState({ icon, title, children, action, bare = false, className, ...props }: EmptyStateProps) {
  return (
    <div {...props} className={cx("omfp-empty", className)} data-bare={bare || undefined}>
      <Icon name={icon} className="omfp-empty__icon" />
      <p className="omfp-empty__title">{title}</p>
      {children !== undefined && <p className="omfp-empty__hint">{children}</p>}
      {action}
    </div>
  );
}
