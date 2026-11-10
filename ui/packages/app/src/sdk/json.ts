/** The app's one guard for untyped JSON (call results typed `unknown`, view manifests): fields stay `unknown`. */
export function isObject(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
