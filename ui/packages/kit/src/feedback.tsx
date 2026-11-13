/** Toasts (UI-160), the message log (UI-522) and empty states (UI-161). */
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

/** The tone of a message: something awaiting the user, a failure, something that went well, anything else. */
export type MessageLogTone = "attention" | "error" | "ok" | "info";

/** One line of a {@link MessageLog}. */
export interface MessageLogEntry {
  readonly key: string | number;
  readonly tone: MessageLogTone;
  /** What kind of message it was, in words (`failed`, `error`, `done`), in the tone's color after `◆`. */
  readonly kind: string;
  /** When, as shown (`14:03:12`), and as a machine-readable date (`dateTime`). */
  readonly time: string;
  readonly dateTime: string;
  /** What it is about (a thread title), before the text. */
  readonly title?: ReactNode;
  readonly text: ReactNode;
}

export interface MessageLogProps extends Omit<HTMLAttributes<HTMLOListElement>, "children"> {
  entries: readonly MessageLogEntry[];
  /** The one muted line of an empty log. */
  emptyText: string;
}

/**
 * Messages that went by (UI-522), like Emacs's `*Messages*`: one line per message in the given order, its time in a
 * column, then `◆` and its kind in the tone's color, its title and its text, wrapping.
 */
export function MessageLog({ entries, emptyText, className, ...props }: MessageLogProps) {
  if (entries.length === 0) {
    return <p className="omfp-messages__empty">{emptyText}</p>;
  }
  return (
    <ol {...props} className={cx("omfp-messages", className)}>
      {entries.map((entry) => (
        <li key={entry.key} className="omfp-messages__line" data-tone={entry.tone} data-testid="message">
          <time className="omfp-messages__time" dateTime={entry.dateTime}>
            {entry.time}
          </time>
          <span className="omfp-messages__body">
            <span className="omfp-messages__kind">
              <span aria-hidden="true">◆ </span>
              {entry.kind}
            </span>{" "}
            {entry.title !== undefined && <span className="omfp-messages__title">{entry.title}</span>}{" "}
            <span className="omfp-messages__text">{entry.text}</span>
          </span>
        </li>
      ))}
    </ol>
  );
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
