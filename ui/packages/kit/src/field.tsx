import type { ReactNode, Ref } from "react";
import {
  Button as AriaButton,
  SearchField as AriaSearchField,
  type SearchFieldProps as AriaSearchFieldProps,
  TextArea as AriaTextArea,
  type TextAreaProps as AriaTextAreaProps,
  TextField as AriaTextField,
  type TextFieldProps as AriaTextFieldProps,
  FieldError,
  Input,
  type InputProps,
  Label,
  Text,
} from "react-aria-components";
import { cx, withClass } from "./class.ts";
import { Icon } from "./icons.tsx";
import { Kbd } from "./kbd.tsx";

/** `data-*` attributes in an object literal (JSX accepts them anyway; spread objects need the index signature). */
export type DataAttributes = { readonly [name: `data-${string}`]: string | number | boolean | undefined };

/** The box around an input (UI-150): an optional leading icon and trailing key chip. */
export interface FieldDecoration {
  /** A leading icon (an `Icon`), in on-surface-variant. */
  icon?: ReactNode;
  /** A trailing key chip, e.g. `/` or `tab`. */
  kbd?: string | undefined;
  /** The code family at fs-xs, for paths and commands. */
  mono?: boolean | undefined;
}

function InputBox({ icon, kbd, mono, children }: FieldDecoration & { children: ReactNode }) {
  return (
    <div className="omfp-input-box" data-mono={mono === true || undefined}>
      {icon}
      {children}
      {kbd !== undefined && <Kbd aria-hidden="true">{kbd}</Kbd>}
    </div>
  );
}

/** Label and help texts shared by the text inputs. */
export interface FieldTexts {
  /** Visible label (also the accessible name). */
  label: string;
  /** Keeps the label for screen readers only (dense layouts with an obvious purpose, e.g. a composer). */
  hideLabel?: boolean;
  description?: ReactNode;
  /** Shown when the field is invalid; passing one marks the field invalid unless `isInvalid` says otherwise. */
  errorMessage?: ReactNode;
}

function FieldLabel({ label, hideLabel }: Pick<FieldTexts, "label" | "hideLabel">) {
  return <Label className={cx("omfp-field__label", hideLabel === true && "omfp-visually-hidden")}>{label}</Label>;
}

function FieldHelp({ description, errorMessage }: Pick<FieldTexts, "description" | "errorMessage">) {
  return (
    <>
      {description !== undefined && (
        <Text slot="description" className="omfp-field__description">
          {description}
        </Text>
      )}
      <FieldError className="omfp-field__error">
        {errorMessage === undefined ? undefined : (
          <>
            <Icon name="circle-alert" />
            {errorMessage}
          </>
        )}
      </FieldError>
    </>
  );
}

export interface TextFieldProps extends Omit<AriaTextFieldProps, "children">, FieldTexts, FieldDecoration {
  placeholder?: string;
  /** Ref to the `<input>` (for focusing it programmatically). */
  inputRef?: Ref<HTMLInputElement>;
  /** Extra attributes for the `<input>` itself (e.g. its `data-testid`); `data-*` on the field go to the wrapper. */
  inputProps?: Omit<InputProps, "className"> & DataAttributes;
}

/** A labelled single-line text input (UI-150). */
export function TextField({
  icon,
  kbd,
  mono,
  label,
  hideLabel,
  description,
  errorMessage,
  placeholder,
  inputRef,
  inputProps,
  className,
  isInvalid,
  ...props
}: TextFieldProps) {
  return (
    <AriaTextField
      {...props}
      isInvalid={isInvalid ?? errorMessage !== undefined}
      className={withClass("omfp-field", className)}
    >
      <FieldLabel label={label} hideLabel={hideLabel === true} />
      <InputBox icon={icon} kbd={kbd} mono={mono === true}>
        <Input
          {...inputProps}
          {...(placeholder === undefined ? {} : { placeholder })}
          ref={inputRef}
          className="omfp-input"
        />
      </InputBox>
      <FieldHelp description={description} errorMessage={errorMessage} />
    </AriaTextField>
  );
}

export interface TextAreaProps extends Omit<AriaTextFieldProps, "children">, FieldTexts {
  placeholder?: string;
  /** Visible rows; the area grows with CSS `field-sizing` where supported. */
  rows?: number;
  /** Ref to the `<textarea>`. */
  inputRef?: Ref<HTMLTextAreaElement>;
  /** Extra attributes for the `<textarea>` itself (e.g. its `data-testid`). */
  inputProps?: Omit<AriaTextAreaProps, "className"> & DataAttributes;
}

/** A labelled multi-line text input. */
export function TextArea({
  label,
  hideLabel,
  description,
  errorMessage,
  placeholder,
  rows,
  inputRef,
  inputProps,
  className,
  isInvalid,
  ...props
}: TextAreaProps) {
  return (
    <AriaTextField
      {...props}
      isInvalid={isInvalid ?? errorMessage !== undefined}
      className={withClass("omfp-field", className)}
    >
      <FieldLabel label={label} hideLabel={hideLabel === true} />
      <InputBox>
        <AriaTextArea
          {...inputProps}
          {...(placeholder === undefined ? {} : { placeholder })}
          {...(rows === undefined ? {} : { rows })}
          ref={inputRef}
          className="omfp-input omfp-textarea"
        />
      </InputBox>
      <FieldHelp description={description} errorMessage={errorMessage} />
    </AriaTextField>
  );
}

export interface SearchFieldProps extends Omit<AriaSearchFieldProps, "children">, FieldTexts {
  placeholder?: string;
  /** Accessible name of the clear button. */
  clearLabel?: string;
  /** `data-testid` of the clear button (`clear` by default). */
  clearTestId?: string;
  inputRef?: Ref<HTMLInputElement>;
  inputProps?: Omit<InputProps, "className"> & DataAttributes;
}

/** A search input (UI-150) with a search icon: Escape clears it, a clear button appears when it has text. */
export function SearchField({
  label,
  hideLabel,
  description,
  errorMessage,
  placeholder,
  clearLabel = "Clear",
  clearTestId = "clear",
  inputRef,
  inputProps,
  className,
  isInvalid,
  ...props
}: SearchFieldProps) {
  return (
    <AriaSearchField
      {...props}
      isInvalid={isInvalid ?? errorMessage !== undefined}
      className={withClass("omfp-field omfp-search", className)}
    >
      <FieldLabel label={label} hideLabel={hideLabel === true} />
      <InputBox icon={<Icon name="search" />}>
        <Input
          {...inputProps}
          {...(placeholder === undefined ? {} : { placeholder })}
          ref={inputRef}
          className="omfp-input"
        />
        <AriaButton className="omfp-search__clear" aria-label={clearLabel} data-testid={clearTestId}>
          <Icon name="x" />
        </AriaButton>
      </InputBox>
      <FieldHelp description={description} errorMessage={errorMessage} />
    </AriaSearchField>
  );
}
