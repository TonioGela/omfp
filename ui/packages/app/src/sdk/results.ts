/**
 * The window's last results of the SDK's data hooks (UI-392): `useCall(method, params)`, `useFile` and `useJob` keep
 * what they last showed per widget instance and request, so a widget mounted again (a view shown again after it was
 * dropped, a widget moved) shows it at once, flagged `refreshing`, while its new request runs. A least recently used
 * cache, bounded by entries and by size.
 */

import type { WidgetInfoScope } from "./widget.ts";

/** How many results the window keeps. */
export const RESULT_ENTRIES = 256;
/** How much the window keeps, in UTF-16 code units of the results' JSON (about 8 MB); a bigger result is not kept. */
export const RESULT_UNITS = 4_000_000;

interface Entry {
  readonly value: unknown;
  readonly size: number;
}

/** The size of `value` as {@link ResultCache} counts it: its JSON's length (`undefined` and functions count 0). */
export function jsonSize(value: unknown): number {
  return JSON.stringify(value)?.length ?? 0;
}

export class ResultCache {
  /** Least recently used first (`Map` keeps insertion order). */
  readonly #entries = new Map<string, Entry>();
  readonly #maxEntries: number;
  readonly #maxSize: number;
  #size = 0;

  constructor(maxEntries = RESULT_ENTRIES, maxSize = RESULT_UNITS) {
    this.#maxEntries = maxEntries;
    this.#maxSize = maxSize;
  }

  /** The value kept under `key`, now the most recently used; `undefined` when there is none. */
  get(key: string): { readonly value: unknown } | undefined {
    const entry = this.#entries.get(key);
    if (entry === undefined) {
      return undefined;
    }
    this.#entries.delete(key);
    this.#entries.set(key, entry);
    return entry;
  }

  /** Keeps `value` under `key` (`size`: its {@link jsonSize} unless given), dropping the least recently used ones
   * over the bounds; a value over the size bound alone is not kept and drops the key's previous one. */
  set(key: string, value: unknown, size = jsonSize(value)): void {
    this.delete(key);
    if (size > this.#maxSize) {
      return;
    }
    this.#entries.set(key, { value, size });
    this.#size += size;
    for (const [oldest, entry] of this.#entries) {
      if (this.#entries.size <= this.#maxEntries && this.#size <= this.#maxSize) {
        break;
      }
      this.#entries.delete(oldest);
      this.#size -= entry.size;
    }
  }

  delete(key: string): void {
    const entry = this.#entries.get(key);
    if (entry !== undefined) {
      this.#entries.delete(key);
      this.#size -= entry.size;
    }
  }

  /** How many values are kept, and their total size. */
  get stats(): { readonly entries: number; readonly size: number } {
    return { entries: this.#entries.size, size: this.#size };
  }
}

/** The cache key of a hook's request: the workspace, widget (reference and instance), what and which. Outside a
 * widget slot the widget parts are empty. */
export function resultKey(scope: WidgetInfoScope | undefined, kind: string, request: string): string {
  return [scope?.workspaceId ?? "", scope?.reference ?? "", scope?.instance ?? "", kind, request].join("\u0000");
}
