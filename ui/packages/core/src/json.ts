/** JSON values as produced by `JSON.parse`. */

export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };

/** A JSON object (`serde_json::Map`). */
export type JsonObject = { [key: string]: JsonValue };

/** `true` for a JSON object: not an array, not `null`. */
export function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** `record` with `key` set to `value`; safe for any key, `__proto__` included. */
export function withKey<T>(record: Record<string, T>, key: string, value: T): Record<string, T> {
  return { ...record, [key]: value };
}

/** `record` without `key`; the same object when the key is absent. */
export function withoutKey<T>(record: Record<string, T>, key: string): Record<string, T> {
  if (!Object.hasOwn(record, key)) {
    return record;
  }
  const copy = { ...record };
  delete copy[key];
  return copy;
}
