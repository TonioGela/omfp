/**
 * HTML snapshots that change only when the markup really changes (TST-032): ids React (`useId`) and React Aria
 * generate differ between runs and test orders, so every `id` becomes `id-<n>` by order of first appearance and the
 * attributes referring to ids (`for`, `aria-labelledby`, `aria-controls`, …) follow; timestamps become `<timestamp>`.
 */

/** Attributes holding one id or a space-separated list of ids. */
const ID_REFERENCES = [
  "for",
  "aria-activedescendant",
  "aria-controls",
  "aria-describedby",
  "aria-details",
  "aria-errormessage",
  "aria-flowto",
  "aria-labelledby",
  "aria-owns",
  "list",
  "form",
  "popovertarget",
] as const;

/** ISO 8601 date-times (`2026-11-10T09:26:50.707Z`, with or without fraction and offset). */
const ISO_TIMESTAMP = /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?/g;
/** Attributes whose whole value is a time. */
const TIME_ATTRIBUTES = ["datetime", "data-timestamp", "data-time"] as const;

/** The normalized `outerHTML` of `root` (left untouched; a clone is rewritten). */
export function snapshotHtml(root: Element): string {
  const clone = root.cloneNode(true) as Element;
  const elements = [clone, ...clone.querySelectorAll("*")];
  const ids = new Map<string, string>();
  const mapped = (id: string) => {
    let name = ids.get(id);
    if (name === undefined) {
      name = `id-${ids.size + 1}`;
      ids.set(id, name);
    }
    return name;
  };
  for (const element of elements) {
    const id = element.getAttribute("id");
    if (id !== null) {
      element.setAttribute("id", mapped(id));
    }
  }
  for (const element of elements) {
    for (const name of ID_REFERENCES) {
      const value = element.getAttribute(name);
      if (value !== null) {
        element.setAttribute(name, value.split(/\s+/).filter(Boolean).map(mapped).join(" "));
      }
    }
    for (const name of TIME_ATTRIBUTES) {
      if (element.hasAttribute(name)) {
        element.setAttribute(name, "<timestamp>");
      }
    }
  }
  return normalizeTimestamps(clone.outerHTML);
}

/** `html` with ISO 8601 timestamps replaced. */
export function normalizeTimestamps(html: string): string {
  return html.replace(ISO_TIMESTAMP, "<timestamp>");
}
