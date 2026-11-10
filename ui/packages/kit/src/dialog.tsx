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

/** A modal dialog: focus moves into it, stays trapped while open and returns to the trigger on close. */
export function Dialog({
  title,
  children,
  isOpen,
  onOpenChange,
  isDismissable = false,
  className,
  ...props
}: DialogProps) {
  return (
    <ModalOverlay {...overlayProps(isOpen, onOpenChange, isDismissable)}>
      <Modal className="omfp-modal">
        <AriaDialog {...props} className={cx("omfp-dialog", className)}>
          {(renderProps) => (
            <>
              <Heading slot="title" className="omfp-dialog__title">
                {title}
              </Heading>
              <div className="omfp-dialog__body">
                {typeof children === "function" ? children(renderProps) : children}
              </div>
            </>
          )}
        </AriaDialog>
      </Modal>
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
  /** Test ids of the dialog and its buttons (`confirm-dialog`, `confirm`, `cancel` by default). */
  testId?: string;
  confirmTestId?: string;
  cancelTestId?: string;
}

/** An alert dialog asking to confirm an action; confirming calls `onConfirm` and closes it. */
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
}: ConfirmDialogProps) {
  const danger = tone === "danger";
  return (
    <Dialog isOpen={isOpen} onOpenChange={onOpenChange} title={title} role="alertdialog" data-testid={testId}>
      {({ close }) => (
        <>
          <div className="omfp-dialog__message">{message}</div>
          <div className="omfp-dialog__actions">
            <Button variant="quiet" onPress={close} autoFocus={danger} data-testid={cancelTestId}>
              {cancelLabel}
            </Button>
            <Button
              variant={danger ? "danger" : "primary"}
              autoFocus={!danger}
              data-testid={confirmTestId}
              onPress={() => {
                onConfirm();
                close();
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
