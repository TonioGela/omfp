/**
 * L2: the contrast guard on `<html>` (UI-196). happy-dom computes no colors, so the guard reads through an injected
 * reader resolving tokens like a browser would: `<html>`'s inline overrides first, else the theme in the page's
 * `<style data-omfp-theme>` in the effective scheme (`data-color-scheme`, or the system's through `matchMedia`).
 */

import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { parseCssColor } from "../../src/theme/contrast.ts";
import { applyContrastGuard, type ColorReader, GUARD_ATTRIBUTE, installContrastGuard } from "../../src/theme/guard.ts";
import { applyUserTheme, THEME_ATTRIBUTE } from "../../src/theme/theme.ts";
import { type Scheme, type ThemeName, themeColors, themeCss } from "./themes.ts";

const THEMES: readonly ThemeName[] = ["synth", "matugen-forest", "dux-solarized"];

/** `matchMedia` answering `(prefers-color-scheme: dark)` from a switch the test flips. */
class SystemScheme {
  dark = false;
  readonly listeners = new Set<() => void>();

  readonly matchMedia = (query: string): MediaQueryList => {
    const system = this;
    const list: Pick<MediaQueryList, "media" | "matches" | "addEventListener" | "removeEventListener"> = {
      media: query,
      get matches() {
        return query === "(prefers-color-scheme: dark)" && system.dark;
      },
      addEventListener: (_type: string, listener: unknown) => system.listeners.add(listener as () => void),
      removeEventListener: (_type: string, listener: unknown) => system.listeners.delete(listener as () => void),
    };
    return list as MediaQueryList;
  };

  flip(dark: boolean): void {
    this.dark = dark;
    for (const listener of this.listeners) {
      listener();
    }
  }
}

function pageReader(root: HTMLElement, system: SystemScheme): ColorReader {
  return (token) => {
    const inline = root.style.getPropertyValue(`--omfp-${token}`);
    if (inline !== "") {
      return parseCssColor(inline);
    }
    const css = document.querySelector(`[${THEME_ATTRIBUTE}]`)?.textContent ?? "";
    const theme = THEMES.find((name) => (themeCss(name) ?? "") === css) ?? "synth";
    const attribute = root.dataset.colorScheme;
    const scheme: Scheme = attribute === "dark" || (attribute !== "light" && system.dark) ? "dark" : "light";
    return themeColors(theme, scheme).get(token);
  };
}

/** Lets the MutationObserver deliver its records (a microtask). */
const settle = async () => {
  await Promise.resolve();
};

describe("contrast guard", () => {
  const root = document.documentElement;
  const original = window.matchMedia;
  let system: SystemScheme;

  beforeEach(() => {
    system = new SystemScheme();
    window.matchMedia = system.matchMedia;
    root.removeAttribute("style");
    root.removeAttribute(GUARD_ATTRIBUTE);
    delete root.dataset.colorScheme;
    for (const element of document.querySelectorAll(`[${THEME_ATTRIBUTE}]`)) {
      element.remove();
    }
  });

  afterEach(() => {
    window.matchMedia = original;
  });

  // spec: UI-196
  test("a failing role gets an inline override on <html>; passing roles and other inline properties stay", () => {
    root.style.setProperty("--omfp-font-code", "Iosevka");
    applyUserTheme(document, themeCss("dux-solarized"));
    root.dataset.colorScheme = "light";
    const adjustments = applyContrastGuard(root, pageReader(root, system));
    expect(adjustments.map(({ role }) => role)).toEqual([
      "tertiary",
      "secondary",
      "error",
      "on-primary",
      "on-tertiary",
      "surface-container-highest",
    ]);
    for (const { role, to } of adjustments) {
      expect(root.style.getPropertyValue(`--omfp-${role}`)).toBe(to);
    }
    expect(root.style.getPropertyValue("--omfp-primary")).toBe("");
    expect(root.style.getPropertyValue("--omfp-font-code")).toBe("Iosevka");
    expect(root.getAttribute(GUARD_ATTRIBUTE)).toBe(
      "tertiary secondary error on-primary on-tertiary surface-container-highest",
    );
  });

  // spec: UI-196, UI-139
  test("Synth renders as it is: no override, no attribute", () => {
    root.dataset.colorScheme = "dark";
    expect(applyContrastGuard(root, pageReader(root, system))).toEqual([]);
    expect(root.getAttribute("style") ?? "").toBe("");
    expect(root.hasAttribute(GUARD_ATTRIBUTE)).toBe(false);
  });

  // spec: UI-196, UI-091
  test("it re-measures from the theme's own colors when the user theme, the scheme or the system scheme changes", async () => {
    applyUserTheme(document, themeCss("dux-solarized"));
    root.dataset.colorScheme = "light";
    const dispose = installContrastGuard(document, root, pageReader(root, system));
    const tertiary = () => root.style.getPropertyValue("--omfp-tertiary");
    const light = tertiary();
    expect(light).not.toBe("");

    // The window forces dark: the dark tertiary is lightened instead.
    root.dataset.colorScheme = "dark";
    await settle();
    const dark = tertiary();
    expect(dark).not.toBe("");
    expect(dark).not.toBe(light);
    expect(root.getAttribute(GUARD_ATTRIBUTE)).toBe("tertiary secondary error outline on-error");

    // Back to the system scheme, which turns dark later.
    root.dataset.colorScheme = "system";
    await settle();
    expect(tertiary()).toBe(light);
    system.flip(true);
    expect(tertiary()).toBe(dark);

    // The user theme is replaced by a good one: every override goes.
    applyUserTheme(document, themeCss("matugen-forest"));
    await settle();
    expect(root.getAttribute("style") ?? "").toBe("");
    expect(root.hasAttribute(GUARD_ATTRIBUTE)).toBe(false);

    // And comes back with the preset; removing the guard clears it.
    applyUserTheme(document, themeCss("dux-solarized"));
    await settle();
    expect(tertiary()).toBe(dark);
    dispose();
    expect(tertiary()).toBe("");
    expect(root.hasAttribute(GUARD_ATTRIBUTE)).toBe(false);

    // Once removed it no longer reacts.
    root.dataset.colorScheme = "light";
    await settle();
    expect(tertiary()).toBe("");
  });

  // spec: UI-196
  test("by default it reads the colors the page computes (custom properties through the theme's cascade)", () => {
    applyUserTheme(document, themeCss("dux-solarized"));
    root.dataset.colorScheme = "light";
    const expected = applyContrastGuard(root, pageReader(root, system));
    const dispose = installContrastGuard(document, root);
    expect(root.getAttribute(GUARD_ATTRIBUTE)).toBe(
      "tertiary secondary error on-primary on-tertiary surface-container-highest",
    );
    for (const { role, to } of expected) {
      expect(root.style.getPropertyValue(`--omfp-${role}`)).toBe(to);
    }
    dispose();
  });
});
