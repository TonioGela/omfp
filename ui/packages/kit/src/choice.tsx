import type { ReactNode } from "react";
import {
  Checkbox as AriaCheckbox,
  type CheckboxProps as AriaCheckboxProps,
  Radio as AriaRadio,
  RadioGroup as AriaRadioGroup,
  type RadioGroupProps as AriaRadioGroupProps,
  type RadioProps as AriaRadioProps,
  FieldError,
  Label,
  Text,
} from "react-aria-components";
import { cx, withClass } from "./class.ts";
import { Icon } from "./icons.tsx";

export interface CheckboxProps extends Omit<AriaCheckboxProps, "children"> {
  /** Visible label next to the box. */
  children?: ReactNode;
}

/** A checkbox with its label (UI-153); Space toggles it. Checked shows a check, mixed a dash. */
export function Checkbox({ className, children, ...props }: CheckboxProps) {
  return (
    <AriaCheckbox {...props} className={withClass("omfp-checkbox", className)}>
      <span className="omfp-checkbox__box" aria-hidden="true">
        <Icon name="check" className="omfp-checkbox__check" />
        <span className="omfp-checkbox__dash" />
      </span>
      {children}
    </AriaCheckbox>
  );
}

export interface RadioGroupProps extends Omit<AriaRadioGroupProps, "children"> {
  label: string;
  hideLabel?: boolean;
  description?: ReactNode;
  errorMessage?: ReactNode;
  children?: ReactNode;
}

/** A labelled group of radios; arrow keys move the selection. */
export function RadioGroup({
  label,
  hideLabel = false,
  description,
  errorMessage,
  className,
  children,
  ...props
}: RadioGroupProps) {
  return (
    <AriaRadioGroup {...props} className={withClass("omfp-field omfp-radio-group", className)}>
      <Label className={cx("omfp-field__label", hideLabel && "omfp-visually-hidden")}>{label}</Label>
      {children}
      {description !== undefined && (
        <Text slot="description" className="omfp-field__description">
          {description}
        </Text>
      )}
      <FieldError className="omfp-field__error">{errorMessage}</FieldError>
    </AriaRadioGroup>
  );
}

export interface RadioProps extends Omit<AriaRadioProps, "children"> {
  children?: ReactNode;
}

export function Radio({ className, children, ...props }: RadioProps) {
  return (
    <AriaRadio {...props} className={withClass("omfp-radio", className)}>
      <span className="omfp-radio__dot" aria-hidden="true" />
      {children}
    </AriaRadio>
  );
}
