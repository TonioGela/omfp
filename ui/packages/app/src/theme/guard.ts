/**
 * The contrast guard (UI-196): whenever the effective theme changes — initial load, the user theme's `<link>` loading
 * or its `<style>` being replaced (UI-091), `data-color-scheme` changing, the system's `prefers-color-scheme`
 * changing — it removes its overrides, measures the guarded roles as rendered (contrast.ts) and sets an inline
 * `--omfp-<role>` on `<html>` for each one under its threshold, like `[ui.fonts]` does (theme.ts). Derived tokens
 * (gradients, hover, selection, diff colors) follow because they are mixed from the roles.
 */

import { type Adjustment, contrastAdjustments, GUARDED_ROLES, parseCssColor, type Rgb } from "./contrast.ts";

/** Reads a color token (name without `--omfp-`) as rendered; `undefined` when it cannot be resolved. */
export type ColorReader = (token: string) => Rgb | undefined;

/** `<html>`'s attribute listing the roles the guard overrides (space-separated), absent when none. */
export const GUARD_ATTRIBUTE = "data-contrast-guard";

const DARK_QUERY = "(prefers-color-scheme: dark)";

/**
 * Removes the previous overrides from `root`, measures with `read` and sets the overrides the theme needs. Returns
 * them (empty: the theme passes as it is).
 */
export function applyContrastGuard(root: HTMLElement, read: ColorReader): Adjustment[] {
  for (const { role } of GUARDED_ROLES) {
    root.style.removeProperty(`--omfp-${role}`);
  }
  const adjustments = contrastAdjustments(read);
  for (const { role, to } of adjustments) {
    root.style.setProperty(`--omfp-${role}`, to);
  }
  if (adjustments.length === 0) {
    root.removeAttribute(GUARD_ATTRIBUTE);
  } else {
    root.setAttribute(GUARD_ATTRIBUTE, adjustments.map(({ role }) => role).join(" "));
  }
  return adjustments;
}

/**
 * Reads tokens through the page: a hidden probe's computed `color`, parsed directly when the browser serializes it in
 * a notation `parseCssColor` knows, else drawn on a 1×1 canvas (any notation the browser can paint).
 */
export function cssColorReader(document: Document): ColorReader {
  let context: CanvasRenderingContext2D | null | undefined;
  return (token) => {
    const probe = document.createElement("span");
    probe.hidden = true;
    probe.style.color = `var(--omfp-${token})`;
    document.body.append(probe);
    const computed = document.defaultView?.getComputedStyle(probe).color ?? "";
    probe.remove();
    const parsed = parseCssColor(computed);
    if (parsed !== undefined || computed === "") {
      return parsed;
    }
    if (context === undefined) {
      const canvas = document.createElement("canvas");
      canvas.width = 1;
      canvas.height = 1;
      context = canvas.getContext("2d", { willReadFrequently: true });
    }
    if (context === null) {
      return undefined;
    }
    context.clearRect(0, 0, 1, 1);
    context.fillStyle = computed;
    context.fillRect(0, 0, 1, 1);
    const [red = 0, green = 0, blue = 0] = context.getImageData(0, 0, 1, 1).data;
    return [red, green, blue];
  };
}

/**
 * Calls `changed` whenever the effective theme may have changed: `<html>`'s `data-color-scheme`, the system's
 * `prefers-color-scheme`, any change under `<head>` (the user theme's `<style>` text or its replacement) and any
 * stylesheet finishing loading. Returns the unsubscribe.
 */
export function watchTheme(document: Document, changed: () => void): () => void {
  const observer = new MutationObserver(changed);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-color-scheme"] });
  observer.observe(document.head, { childList: true, subtree: true, characterData: true });
  const loaded = (event: Event) => {
    if (event.target instanceof HTMLLinkElement) {
      changed();
    }
  };
  document.addEventListener("load", loaded, true);
  const dark = document.defaultView?.matchMedia(DARK_QUERY);
  dark?.addEventListener("change", changed);
  return () => {
    observer.disconnect();
    document.removeEventListener("load", loaded, true);
    dark?.removeEventListener("change", changed);
  };
}

/** Guards `root` now and after every theme change until the returned function is called (which also clears it). */
export function installContrastGuard(
  document: Document,
  root: HTMLElement = document.documentElement,
  read: ColorReader = cssColorReader(document),
): () => void {
  const guard = () => applyContrastGuard(root, read);
  guard();
  const unwatch = watchTheme(document, guard);
  return () => {
    unwatch();
    applyContrastGuard(root, () => undefined);
  };
}
