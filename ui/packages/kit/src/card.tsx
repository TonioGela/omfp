import { type HTMLAttributes, type ReactNode, useId } from "react";
import { Button as AriaButton } from "react-aria-components";
import { cx } from "./class.ts";

export type CardTone = "default" | "error" | "warning" | "success";

export interface CardCollapsible {
  expanded: boolean;
  onExpandedChange: (expanded: boolean) => void;
}

export interface CardProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  /** Header text; with `collapsible` it becomes the toggle button. */
  title?: ReactNode;
  /** Controls at the end of the header (buttons, badges). */
  actions?: ReactNode;
  tone?: CardTone;
  /** Makes the body expandable (controlled); the title button toggles it. */
  collapsible?: CardCollapsible;
  /** `data-testid` of the collapse toggle (`toggle` by default). */
  toggleTestId?: string;
  children?: ReactNode;
}

/** A bordered surface with an optional header (title + actions), toned for status, optionally collapsible. */
export function Card({
  title,
  actions,
  tone = "default",
  collapsible,
  toggleTestId = "toggle",
  className,
  children,
  ...props
}: CardProps) {
  const bodyId = useId();
  const expanded = collapsible === undefined || collapsible.expanded;
  const hasHeader = title !== undefined || actions !== undefined || collapsible !== undefined;
  return (
    <section {...props} className={cx("omfp-card", className)} data-tone={tone} data-expanded={expanded}>
      {hasHeader && (
        <header className="omfp-card__header">
          {collapsible === undefined ? (
            <div className="omfp-card__title">{title}</div>
          ) : (
            <AriaButton
              className="omfp-card__toggle"
              aria-expanded={collapsible.expanded}
              aria-controls={bodyId}
              onPress={() => collapsible.onExpandedChange(!collapsible.expanded)}
              data-testid={toggleTestId}
            >
              <span className="omfp-card__chevron" aria-hidden="true" />
              <span className="omfp-card__title">{title}</span>
            </AriaButton>
          )}
          {actions !== undefined && <div className="omfp-card__actions">{actions}</div>}
        </header>
      )}
      <div id={bodyId} className="omfp-card__body" hidden={!expanded}>
        {expanded && children}
      </div>
    </section>
  );
}
