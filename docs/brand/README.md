# omfp logo

`omfπ` set in Commit Mono Bold as outlines: `omf` in the text colour, `π` (omp is oh-my-pi) in the gradient
`#16b3cc` → `#6a5cf0` (62 %) → `#8a6cf0`, left to right. Requirements: UI-220..222 in
[09-design.md](../specs/09-design.md#logo).

| File | Use |
| --- | --- |
| `wordmark.svg` | Wordmark, text in `currentColor` |
| `wordmark-light.svg`, `wordmark-dark.svg` | Wordmark with fixed text `#1a1b23` / `#e2e1ec` (a README's `<picture>`) |
| `mark.svg` | Mark `[π]`, brackets in `currentColor` at 62 % |
| `mark-light.svg`, `mark-dark.svg` | Mark with fixed bracket colours |
| `icon.svg`, `icon-32.png` | `π` alone, for 16–32 px (the web favicon) |
| `screenshot-light.png`, `screenshot-dark.png` | The app, for the repository README |

The UI draws the same outlines inline (`ui/packages/kit/src/logo.tsx`) with the theme's gradient instead of the fixed
one. The desktop icon is `crates/omfp/icons/icon.png`.

## Regenerate

`generate.py` reads the bundled `ui/packages/kit/fonts/commit-mono-latin-700-normal.woff2` with fontTools and writes
the SVGs here, the kit's `logo-paths.ts` and the desktop icon's `icon.html`; `just brand` runs it and then renders
`icon-32.png` and `icon.png` with headless Chromium. From the repository root:

```sh
nix develop -c just brand
```

The generator alone (fontTools with brotli for woff2):

```sh
nix shell --impure --expr 'with import <nixpkgs> {}; python3.withPackages (p: [p.fonttools p.brotli])' \
  -c python3 docs/brand/generate.py
```

The screenshots come from the headless E2E journey `cargo test -p e2e --test screenshots a_busy_thread` (after
`just build-web`, with `OMFP_SCREENSHOT_DIR` set): `omfp-a-thread-light.png` and `omfp-b-thread-dark.png`.

## Licence

The glyph outlines come from Commit Mono, © 2023 Commit Mono authors, under the SIL Open Font License 1.1
([commit-mono-OFL.txt](../../ui/packages/kit/fonts/commit-mono-OFL.txt)). The OFL covers the font software; artwork
made with the font, such as this logo as outlines, is not a Modified Version of it and may be used freely. The rest
of omfp is GPL-3.0-or-later.
