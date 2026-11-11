/** The bundle's `index.html` (build.ts) and the favicon files it links. */

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
 * The favicon, the import map, the bundle's CSS, then the user theme (`/theme.css`, UI-091, so its tokens win), and
 * the app entry.
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
    '<meta name="viewport" content="width=device-width, initial-scale=1">',
    "<title>omfp</title>",
    ...FAVICONS.map(({ link }) => link),
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
