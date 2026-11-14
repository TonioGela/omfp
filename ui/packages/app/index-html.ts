/** The bundle's `index.html` (build.ts), the favicon files it links and its web app manifest. */

/** The favicon (UI-222): the logo's `π` (UI-220) from docs/brand (`just brand`), copied into `dist/` as `name`. */
export const FAVICONS: readonly { source: string; name: string; link: string }[] = [
  {
    source: "docs/brand/icon-32.png",
    name: "icon-32.png",
    link: '<link rel="icon" href="/icon-32.png" type="image/png" sizes="32x32">',
  },
  {
    source: "docs/brand/icon.svg",
    name: "icon.svg",
    link: '<link rel="icon" href="/icon.svg" type="image/svg+xml" sizes="any">',
  },
];

/**
 * The installed web app's icons (UI-425): the desktop icon's mark `[π]` (UI-222) on a transparent square, rendered by
 * `just brand` at the two sizes Chromium asks of an installable app, copied into `dist/` as `name`.
 */
export const APP_ICONS: readonly { source: string; name: string; size: number }[] = [
  { source: "docs/brand/app-icon-192.png", name: "app-icon-192.png", size: 192 },
  { source: "crates/omfp/icons/icon.png", name: "app-icon-512.png", size: 512 },
];

/** Where the bundle serves {@link webManifest}. */
export const MANIFEST_NAME = "manifest.webmanifest";

/** A web app manifest, as far as the bundle writes one. */
export interface WebManifest {
  readonly id: string;
  readonly name: string;
  readonly short_name: string;
  readonly start_url: string;
  readonly scope: string;
  readonly display: "standalone";
  readonly icons: readonly { src: string; type: string; sizes: string }[];
}

/**
 * The web app manifest (UI-425): installed from a phone's browser, the app opens `standalone`, in a window of its own
 * without the browser's title and URL bar, at the daemon's root (the token stays in the page's storage, UI-002).
 */
export function webManifest(): WebManifest {
  return {
    id: "/",
    name: "omfp",
    short_name: "omfp",
    start_url: "/",
    scope: "/",
    display: "standalone",
    icons: APP_ICONS.map(({ name, size }) => ({ src: `/${name}`, type: "image/png", sizes: `${size}x${size}` })),
  };
}

/**
 * The favicon, the web app manifest, the import map, the bundle's CSS, then the user theme (`/theme.css`, UI-091, so its
 * tokens win), and the app entry. The on-screen keyboard resizes the layout viewport
 * (`interactive-widget=resizes-content`, UI-424), so the app, as tall as the window, ends above it instead of being
 * panned out of view.
 */
export function indexHtml({
  importMap,
  styles,
  main,
}: {
  importMap: { imports: Record<string, string> };
  /** URLs of the bundle's stylesheets. */
  styles: readonly string[];
  /** URL of the app entry. */
  main: string;
}): string {
  return [
    "<!doctype html>",
    '<html lang="en">',
    "<head>",
    '<meta charset="utf-8">',
    '<meta name="viewport" content="width=device-width, initial-scale=1, interactive-widget=resizes-content">',
    "<title>omfp</title>",
    ...FAVICONS.map(({ link }) => link),
    `<link rel="manifest" href="/${MANIFEST_NAME}">`,
    `<script type="importmap">${JSON.stringify(importMap)}</script>`,
    ...styles.map((href) => `<link rel="stylesheet" href="${href}">`),
    '<link rel="stylesheet" href="/theme.css" data-omfp-theme>',
    "</head>",
    "<body>",
    '<div id="root"></div>',
    `<script type="module" src="${main}"></script>`,
    "</body>",
    "</html>",
    "",
  ].join("\n");
}
