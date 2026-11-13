/**
 * The full-screen sheet of a phone (UI-461, UI-462): a modal dialog covering the window (the visual viewport on a touch
 * screen), its title and a close button on top, an optional search field under them, then sections of touch-sized
 * rows that scroll on their own. Escape and the close button close it; the focus goes back where it was.
 */

import { type HTMLAttributes, type ReactNode, useId } from "react";
import {
  Button as AriaButton,
  type ButtonProps as AriaButtonProps,
  Dialog,
  Modal,
  ModalOverlay,
} from "react-aria-components";
import { IconButton } from "./button.tsx";
import { cx, withClass } from "./class.ts";
import { Icon } from "./icons.tsx";

export interface SheetProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  /** The title on top, also the dialog's accessible name. */
  title: string;
  /** The accessible name of the close button. */
  closeLabel: string;
  /** Escape or the close button. */
  onClose?: () => void;
  /** The search field under the title. */
  search?: ReactNode;
  /** Renders in place, without the modal overlay (the `/kit` gallery). */
  inline?: boolean;
  /** {@link SheetSection}s, or an empty-state line. */
  children?: ReactNode;
}

export function Sheet({
  title,
  closeLabel,
  onClose,
  search,
  inline = false,
  className,
  children,
  ...props
}: SheetProps) {
  const dialog = (
    <Dialog aria-label={title} className="omfp-sheet">
      <div {...props} className={cx("omfp-sheet__frame", className)}>
        <header className="omfp-sheet__head">
          <h2 className="omfp-sheet__title">{title}</h2>
          <IconButton
            label={closeLabel}
            icon={<Icon name="x" />}
            className="omfp-sheet__close"
            data-testid="sheet-close"
            onPress={() => onClose?.()}
          />
        </header>
        {search !== undefined && <div className="omfp-sheet__search">{search}</div>}
        {/* biome-ignore lint/a11y/noNoninteractiveTabindex: a scrolling region the keyboard must reach to scroll (UI-092). */}
        <section className="omfp-sheet__list omfp-focus-inset" aria-label={title} tabIndex={0}>
          {children}
        </section>
      </div>
    </Dialog>
  );
  if (inline) {
    return <div className="omfp-sheet-modal">{dialog}</div>;
  }
  return (
    <ModalOverlay
      isOpen
      onOpenChange={(open) => {
        if (!open) {
          onClose?.();
        }
      }}
      className="omfp-sheet-overlay"
    >
      <Modal className="omfp-sheet-modal">{dialog}</Modal>
    </ModalOverlay>
  );
}

export interface SheetSectionProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  /** The section's heading. */
  title: ReactNode;
  /** {@link SheetRow}s. */
  children?: ReactNode;
}

/** One group of a sheet: its heading, then its rows. */
export function SheetSection({ title, className, children, ...props }: SheetSectionProps) {
  const heading = useId();
  return (
    <section {...props} className={cx("omfp-sheet__section", className)} aria-labelledby={heading}>
      <h3 id={heading} className="omfp-sheet__heading">
        {title}
      </h3>
      {children}
    </section>
  );
}

export interface SheetRowProps extends Omit<AriaButtonProps, "children"> {
  /** What it does, in a few words: the row's first line. */
  label: string;
  /** What it is called (`:theme`): the smaller, muted second line. */
  name: string;
  /** It leads on to more choices rather than acting at once: a trailing chevron. */
  more?: boolean;
}

/** A touch-sized row of a sheet (`--omfp-touch` tall at least): the label, its name under it. */
export function SheetRow({ label, name, more = false, className, ...props }: SheetRowProps) {
  return (
    <AriaButton
      {...props}
      className={withClass("omfp-sheet__row omfp-focus-inset", className)}
      data-more={more || undefined}
    >
      <span className="omfp-sheet__text">
        <span className="omfp-sheet__label">{label}</span>
        <span className="omfp-sheet__name">{name}</span>
      </span>
      {more && <Icon name="chevron-right" className="omfp-sheet__more" />}
    </AriaButton>
  );
}
