import type { ReactNode } from "react";
import {
  Dialog as AriaDialog,
  type DialogProps as AriaDialogProps,
  Popover as AriaPopover,
  type PopoverProps as AriaPopoverProps,
  type DialogRenderProps,
  Heading,
  Modal,
  ModalOverlay,
  type ModalOverlayProps,
} from "react-aria-components";
import { Button } from "./button.tsx";
import { cx, withClass } from "./class.ts";
import { Icon } from "./icons.tsx";

export { DialogTrigger } from "react-aria-components";

export interface DialogProps extends Omit<AriaDialogProps, "children"> {
  /** Heading; it names the dialog. */
  title: ReactNode;
  /** Body; a function receives `{ close }`. */
  children?: ReactNode | ((opts: DialogRenderProps) => ReactNode);
  /** Controlled open state; omit inside a `DialogTrigger`. */
  isOpen?: boolean;
  onOpenChange?: (isOpen: boolean) => void;
  /** Closes on a click outside (Escape always closes). */
  isDismissable?: boolean;
  /** Renders the dialog surface in place, without the modal overlay (previews such as the `/kit` gallery). */
  inline?: boolean;
}

function overlayProps(
  isOpen: boolean | undefined,
  onOpenChange: ((isOpen: boolean) => void) | undefined,
  isDismissable: boolean,
): ModalOverlayProps {
  const props: ModalOverlayProps = { isDismissable, className: "omfp-modal-overlay" };
  if (isOpen !== undefined) props.isOpen = isOpen;
  if (onOpenChange !== undefined) props.onOpenChange = onOpenChange;
  return props;
}

/**
 * A modal dialog (UI-156): centred on the scrim, focus moves into it, stays trapped while open and returns to the
 * trigger on close.
 */
export function Dialog({
  title,
  children,
  isOpen,
  onOpenChange,
  isDismissable = false,
  inline = false,
  className,
  ...props
}: DialogProps) {
  const dialog = (
    <AriaDialog {...props} className={cx("omfp-dialog", className)}>
      {(renderProps) => (
        <>
          <Heading slot="title" level={2} className="omfp-dialog__title">
            {title}
          </Heading>
          <div className="omfp-dialog__body">{typeof children === "function" ? children(renderProps) : children}</div>
        </>
      )}
    </AriaDialog>
  );
  if (inline) {
    return <div className="omfp-modal">{dialog}</div>;
  }
  return (
    <ModalOverlay {...overlayProps(isOpen, onOpenChange, isDismissable)}>
      <Modal className="omfp-modal">{dialog}</Modal>
    </ModalOverlay>
  );
}

export interface ConfirmDialogProps {
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
  title: ReactNode;
  message: ReactNode;
  confirmLabel: string;
  cancelLabel?: string;
  /** `danger` styles the confirm button as destructive and focuses Cancel first. */
  tone?: "danger";
  onConfirm: () => void;
  /** Key chips of the actions (UI-148, e.g. `esc` and `enter`): the keymap's labels, so a rebinding updates them. */
  cancelKbd?: string;
  confirmKbd?: string;
  /** A leading icon of the confirm button (an `Icon`). */
  confirmIcon?: ReactNode;
  /** Confirming: the confirm button is busy, and the dialog stays open until the caller closes it. */
  busy?: boolean;
  /** Why confirming failed, shown above the actions. */
  error?: ReactNode;
  /** In place, without the overlay (see {@link DialogProps.inline}). */
  inline?: boolean;
  /** Content between the message and the actions (e.g. an option checkbox). */
  children?: ReactNode;
  /** Test ids of the dialog and its buttons (`confirm-dialog`, `confirm`, `cancel` by default). */
  testId?: string;
  confirmTestId?: string;
  cancelTestId?: string;
}

/**
 * An alert dialog asking to confirm an action (UI-156); confirming calls `onConfirm` and closes it, unless the
 * dialog is `busy`-driven: then the caller closes it when the action settles.
 */
export function ConfirmDialog({
  isOpen,
  onOpenChange,
  title,
  message,
  confirmLabel,
  cancelLabel = "Cancel",
  tone,
  onConfirm,
  testId = "confirm-dialog",
  confirmTestId = "confirm",
  cancelTestId = "cancel",
  cancelKbd,
  confirmKbd,
  confirmIcon,
  busy,
  error,
  inline = false,
  children,
}: ConfirmDialogProps) {
  const danger = tone === "danger";
  return (
    <Dialog
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      title={title}
      role="alertdialog"
      data-testid={testId}
      inline={inline}
    >
      {({ close }) => (
        <>
          <div className="omfp-dialog__message">{message}</div>
          {children}
          {error !== undefined && (
            <p className="omfp-dialog__error" role="alert">
              <Icon name="circle-alert" />
              {error}
            </p>
          )}
          <div className="omfp-dialog__actions">
            <Button variant="quiet" onPress={close} autoFocus={danger} kbd={cancelKbd} data-testid={cancelTestId}>
              {cancelLabel}
            </Button>
            <Button
              variant={danger ? "danger" : "primary"}
              autoFocus={!danger}
              icon={confirmIcon}
              kbd={confirmKbd}
              isPending={busy === true}
              data-testid={confirmTestId}
              onPress={() => {
                onConfirm();
                if (busy === undefined) {
                  close();
                }
              }}
            >
              {confirmLabel}
            </Button>
          </div>
        </>
      )}
    </Dialog>
  );
}

export type PopoverProps = AriaPopoverProps;

/** A non-modal overlay anchored to its trigger (menus, selects, completions). */
export function Popover({ className, ...props }: PopoverProps) {
  return <AriaPopover {...props} className={withClass("omfp-popover", className)} />;
}
