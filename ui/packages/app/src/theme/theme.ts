/**
 * Theme, density and fonts on the document (UI-091, UI-094, UI-097). The built-in light and dark tokens follow
 * `prefers-color-scheme` unless `<html data-color-scheme="light|dark">` (window-local) overrides it; `data-density`
 * (the daemon's `[ui] density`) picks compact or normal spacing; `[ui.fonts]` set `--omfp-font-prose|ui|code` inline
 * on `<html>`, which beats any stylesheet, user theme included. The user theme is the daemon's (global,
 * `theme.get`/`theme.set`): the page loads it first through `<link rel="stylesheet" href="/theme.css"
 * data-omfp-theme>` (so it applies before the socket connects), then the `index` topic's `theme` replaces that link
 * with `<style data-omfp-theme>` in the same place and keeps it current (`theme_changed`). The tokens live in
 * `@layer omfp-tokens`, so the unlayered user theme always wins over them.
 */

import { type Density, type FontSettings, INDEX_TOPIC } from "@omfp/protocol";
import { useEffect } from "react";
import {
  COLOR_SCHEME,
  type ColorScheme,
  DEFAULT_DENSITY,
  useSettings,
  useSnapshot,
  useWindowState,
} from "../sdk/index.ts";

export const THEME_ATTRIBUTE = "data-omfp-theme";

const FONT_ROLES = ["prose", "ui", "code"] as const;

export function applyAppearance(
  root: HTMLElement,
  scheme: ColorScheme,
  density: Density,
  fonts: FontSettings | null | undefined,
): void {
  root.dataset.colorScheme = scheme;
  root.dataset.density = density;
  for (const role of FONT_ROLES) {
    const family = fonts?.[role];
    if (family === undefined || family === null || family === "") {
      root.style.removeProperty(`--omfp-font-${role}`);
    } else {
      root.style.setProperty(`--omfp-font-${role}`, family);
    }
  }
}

/** Puts `css` (`null`: no user theme) in the page's `<style data-omfp-theme>`, which replaces the bundle's `<link>`. */
export function applyUserTheme(document: Document, css: string | null): void {
  const current = document.querySelector(`[${THEME_ATTRIBUTE}]`);
  if (current instanceof HTMLStyleElement) {
    if (current.textContent !== (css ?? "")) {
      current.textContent = css ?? "";
    }
    return;
  }
  const style = document.createElement("style");
  style.setAttribute(THEME_ATTRIBUTE, "");
  style.textContent = css ?? "";
  if (current === null) {
    document.head.append(style);
  } else {
    current.replaceWith(style);
  }
}

/** Keeps `<html>`'s colour scheme (window state), density and fonts (daemon settings) current. */
export function useAppearance(root: HTMLElement = document.documentElement): void {
  const [scheme] = useWindowState(COLOR_SCHEME);
  const { ui } = useSettings();
  const density = ui.density ?? DEFAULT_DENSITY;
  const { fonts } = ui;
  useEffect(() => applyAppearance(root, scheme, density, fonts), [root, scheme, density, fonts]);
}

/** Applies the daemon's theme once the `index` topic is live, and every change after (`theme_changed`). */
export function useUserTheme(): void {
  const live = useSnapshot((snapshot) =>
    snapshot.topics.some((topic) => topic.topic === INDEX_TOPIC && topic.sync.kind === "live"),
  );
  const css = useSnapshot((snapshot) => snapshot.index.theme.css);
  useEffect(() => {
    if (live) {
      applyUserTheme(document, css);
    }
  }, [live, css]);
}
