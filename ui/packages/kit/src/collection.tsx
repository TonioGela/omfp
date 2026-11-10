import type { ReactNode } from "react";
import {
  Button as AriaButton,
  ListBox as AriaListBox,
  ListBoxItem as AriaListBoxItem,
  type ListBoxItemProps as AriaListBoxItemProps,
  type ListBoxProps as AriaListBoxProps,
  Menu as AriaMenu,
  MenuItem as AriaMenuItem,
  type MenuItemProps as AriaMenuItemProps,
  type MenuProps as AriaMenuProps,
  MenuSection as AriaMenuSection,
  type MenuSectionProps as AriaMenuSectionProps,
  Select as AriaSelect,
  type SelectProps as AriaSelectProps,
  Separator as AriaSeparator,
  type SeparatorProps as AriaSeparatorProps,
  Collection,
  Header,
  type Key,
  Label,
  SelectValue,
} from "react-aria-components";
import { cx, withClass } from "./class.ts";
import { Popover } from "./dialog.tsx";

export { type Key, MenuTrigger, SubmenuTrigger } from "react-aria-components";

export type ListBoxProps<T extends object> = AriaListBoxProps<T>;

/** A keyboard-navigable list of selectable options (arrow keys, typeahead, `aria-selected`). */
export function ListBox<T extends object>({ className, ...props }: ListBoxProps<T>) {
  return <AriaListBox {...props} className={withClass("omfp-listbox", className)} />;
}

export type ListBoxItemProps<T extends object> = AriaListBoxItemProps<T>;

export function ListBoxItem<T extends object>({ className, ...props }: ListBoxItemProps<T>) {
  return <AriaListBoxItem {...props} className={withClass("omfp-listbox__item", className)} />;
}

export type MenuProps<T extends object> = AriaMenuProps<T>;

/** A menu of actions; put it in a `Popover` inside a `MenuTrigger`. */
export function Menu<T extends object>({ className, ...props }: MenuProps<T>) {
  return <AriaMenu {...props} className={withClass("omfp-menu", className)} />;
}

export type MenuItemProps<T extends object> = AriaMenuItemProps<T>;

export function MenuItem<T extends object>({ className, ...props }: MenuItemProps<T>) {
  return <AriaMenuItem {...props} className={withClass("omfp-menu__item", className)} />;
}

export interface MenuSectionProps<T extends object> extends AriaMenuSectionProps<T> {
  /** Visible section heading. */
  title?: ReactNode;
}

export function MenuSection<T extends object>({ title, className, children, items, ...props }: MenuSectionProps<T>) {
  return (
    <AriaMenuSection {...props} className={cx("omfp-menu__section", className)}>
      {title !== undefined && <Header className="omfp-menu__header">{title}</Header>}
      {typeof children === "function" ? (
        <Collection {...(items === undefined ? {} : { items })}>{children}</Collection>
      ) : (
        children
      )}
    </AriaMenuSection>
  );
}

export type SeparatorProps = AriaSeparatorProps;

/** A divider between menu sections or toolbar groups. */
export function Separator({ className, ...props }: SeparatorProps) {
  return <AriaSeparator {...props} className={cx("omfp-separator", className)} />;
}

export interface SelectItem {
  id: Key;
  label: string;
}

export interface SelectProps
  extends Omit<AriaSelectProps<SelectItem>, "children" | "value" | "defaultValue" | "onChange" | "selectionMode"> {
  label: string;
  hideLabel?: boolean;
  items: readonly SelectItem[];
  selectedKey: Key | null;
  onSelectionChange: (key: Key | null) => void;
}

/** A labelled single-choice picker: a button showing the value, a listbox in a popover. */
export function Select({
  label,
  hideLabel = false,
  items,
  selectedKey,
  onSelectionChange,
  className,
  ...props
}: SelectProps) {
  return (
    <AriaSelect<SelectItem>
      {...props}
      value={selectedKey}
      onChange={onSelectionChange}
      className={withClass("omfp-field omfp-select", className)}
    >
      <Label className={cx("omfp-field__label", hideLabel && "omfp-visually-hidden")}>{label}</Label>
      <AriaButton className="omfp-select__button">
        <SelectValue className="omfp-select__value" />
        <span className="omfp-select__chevron" aria-hidden="true" />
      </AriaButton>
      <Popover>
        <ListBox<SelectItem> items={items}>
          {(item) => (
            <ListBoxItem textValue={item.label} data-testid="select-option" data-key={String(item.id)}>
              {item.label}
            </ListBoxItem>
          )}
        </ListBox>
      </Popover>
    </AriaSelect>
  );
}
