import type { ClassNameOrFunction } from "react-aria-components";

/** Joins the truthy class names with spaces. */
export function cx(...parts: readonly (string | false | null | undefined)[]): string {
  return parts.filter((part): part is string => typeof part === "string" && part !== "").join(" ");
}

/** Prepends the kit's `base` class to a React Aria `className` (a string or a render-props function). */
export function withClass<T>(base: string, className: ClassNameOrFunction<T> | undefined): ClassNameOrFunction<T> {
  if (typeof className === "function") {
    return (values) => cx(base, className(values));
  }
  return cx(base, className);
}
