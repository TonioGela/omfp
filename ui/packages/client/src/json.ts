/** Helpers over JSON-decoded values. */

/** A JSON object (not an array, not `null`). */
export function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Structural equality of JSON-like values (objects compare by own keys; `undefined` members count as absent). */
export function jsonEqual(a: unknown, b: unknown): boolean {
  if (a === b) {
    return true;
  }
  if (Array.isArray(a)) {
    return Array.isArray(b) && a.length === b.length && a.every((item, at) => jsonEqual(item, b[at]));
  }
  if (!isObject(a) || !isObject(b)) {
    return false;
  }
  const keys = (object: Record<string, unknown>) => Object.keys(object).filter((key) => object[key] !== undefined);
  const aKeys = keys(a);
  return aKeys.length === keys(b).length && aKeys.every((key) => jsonEqual(a[key], b[key]));
}
