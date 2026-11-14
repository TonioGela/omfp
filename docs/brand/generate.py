"""The omfp logo (UI-220) from the bundled Commit Mono Bold: glyph outlines as SVG paths, so no file needs the font.

Writes, from the repository root (`just brand` runs it, then renders the PNG images):
- docs/brand/wordmark.svg, mark.svg, icon.svg: `omfπ`, `[π]` and `π`; text in `currentColor`, `π` in the fixed
  gradient; wordmark-light/-dark.svg and mark-light/-dark.svg: the same with fixed text colours for places without
  `currentColor` (a README's `<picture>`);
- ui/packages/kit/src/logo-paths.ts: the outlines the kit draws inline with the theme's gradient (Wordmark, LogoMark,
  LogoIcon);
- crates/omfp/icons/icon.html: the desktop app icon, rendered into icon.png by `just brand`.

Run it with fontTools (and brotli for woff2), e.g.:
  nix shell --impure --expr 'with import <nixpkgs> {}; python3.withPackages (p: [p.fonttools p.brotli])' \
    -c python3 docs/brand/generate.py
"""

from pathlib import Path

from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parents[2]
FONT = ROOT / "ui/packages/kit/fonts/commit-mono-latin-700-normal.woff2"
BRAND = ROOT / "docs/brand"
KIT_MODULE = ROOT / "ui/packages/kit/src/logo-paths.ts"
APP_ICON = ROOT / "crates/omfp/icons/icon.html"

# The fixed gradient of the files: the kit's `--omfp-grad` shape (primary → tertiary at 62 % → end) in tones that read
# on light and dark backgrounds.
STOPS = (("0", "#16b3cc"), ("0.62", "#6a5cf0"), ("1", "#8a6cf0"))
LIGHT_TEXT = "#1a1b23"
DARK_TEXT = "#e2e1ec"
# The brackets of the mark: the text colour at this opacity.
MUTED = "0.62"
# The app icon's brackets: a mid grey that reads on light and dark launchers.
ICON_GREY = "#7a7a8c"
# Space around the ink of the files, in font units (1000 per em).
PAD = 40

font = TTFont(FONT)
glyphs = font.getGlyphSet()
cmap = font.getBestCmap()


def run(text):
    """Each character's path (baseline at y = 0, y down) and the ink bounds of the run."""
    paths = []
    x_min = y_min = float("inf")
    x_max = y_max = float("-inf")
    x = 0
    for char in text:
        glyph = glyphs[cmap[ord(char)]]
        pen = SVGPathPen(glyphs)
        glyph.draw(TransformPen(pen, (1, 0, 0, -1, x, 0)))
        bounds = BoundsPen(glyphs)
        glyph.draw(bounds)
        left, bottom, right, top = bounds.bounds
        paths.append((pen.getCommands(), x + left, x + right))
        x_min, x_max = min(x_min, x + left), max(x_max, x + right)
        y_min, y_max = min(y_min, -top), max(y_max, -bottom)
        x += glyph.width
    return paths, (x_min, y_min, x_max - x_min, y_max - y_min)


def view_box(box, pad=0, square=False):
    x, y, w, h = box
    if square:
        side = max(w, h)
        x, y, w, h = x - (side - w) / 2, y - (side - h) / 2, side, side
    return tuple(round(v) for v in (x - pad, y - pad, w + 2 * pad, h + 2 * pad))


def gradient(x1, x2):
    stops = "".join(f'<stop offset="{offset}" stop-color="{color}"/>' for offset, color in STOPS)
    return (
        f'<linearGradient id="omfp-grad" gradientUnits="userSpaceOnUse" x1="{round(x1)}" y1="0" x2="{round(x2)}"'
        f' y2="0">{stops}</linearGradient>'
    )


def svg(box, body, defs, title, sized=True):
    """An SVG document; `sized` gives it a width and height (a tenth of the font units) for `<img>` without CSS."""
    x, y, w, h = box
    size = f' width="{round(w / 10)}" height="{round(h / 10)}"' if sized else ""
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x} {y} {w} {h}"{size} role="img"><title>{title}</title>'
        f"<defs>{defs}</defs>{body}</svg>\n"
    )


word, word_ink = run("omfπ")
mark, mark_ink = run("[π]")
pi, pi_ink = run("π")

WORD_BOX = view_box(word_ink, PAD)
MARK_BOX = view_box(mark_ink, PAD)
ICON_BOX = view_box(pi_ink, PAD, square=True)


def wordmark(text):
    body = "".join(f'<path d="{d}" fill="{text}"/>' for d, _, _ in word[:3])
    d, left, right = word[3]
    body += f'<path d="{d}" fill="url(#omfp-grad)"/>'
    return svg(WORD_BOX, body, gradient(left, right), "omfp")


def mark_svg(text):
    body = "".join(f'<path d="{d}" fill="{text}" opacity="{MUTED}"/>' for d, _, _ in (mark[0], mark[2]))
    d, left, right = mark[1]
    body += f'<path d="{d}" fill="url(#omfp-grad)"/>'
    return svg(MARK_BOX, body, gradient(left, right), "omfp")


def icon_svg():
    d, left, right = pi[0]
    return svg(ICON_BOX, f'<path d="{d}" fill="url(#omfp-grad)"/>', gradient(left, right), "omfp", sized=False)


files = {
    "wordmark.svg": wordmark("currentColor"),
    "wordmark-light.svg": wordmark(LIGHT_TEXT),
    "wordmark-dark.svg": wordmark(DARK_TEXT),
    "mark.svg": mark_svg("currentColor"),
    "mark-light.svg": mark_svg(LIGHT_TEXT),
    "mark-dark.svg": mark_svg(DARK_TEXT),
    "icon.svg": icon_svg(),
}
for name, content in files.items():
    (BRAND / name).write_text(content)


def ts_string(name, doc, value):
    line = f"export const {name} = {value};"
    return f"/** {doc} */\n" + (line if len(line) <= 120 else f"export const {name} =\n  {value};") + "\n"


def ts_paths(name, doc, paths):
    items = "".join(f'  "{d}",\n' for d in paths)
    return f"/** {doc} */\nexport const {name}: readonly string[] = [\n{items}];\n"


def box_text(box, pad=0, square=False):
    return '"' + " ".join(str(v) for v in view_box(box, pad, square)) + '"'


# The kit's boxes hug the ink: the wordmark's bottom is the baseline (plus the `o`'s overshoot).
KIT_MODULE.write_text(
    "/**\n"
    " * The logo's outlines (UI-220): Commit Mono Bold glyphs as SVG paths in font units (1000 per em, baseline at\n"
    " * y = 0). Generated by docs/brand/generate.py (`just brand`); do not edit.\n"
    " */\n\n"
    + ts_string("WORDMARK_VIEW_BOX", "`omfπ`'s ink box.", box_text(word_ink))
    + "\n"
    + ts_paths("WORDMARK_TEXT", "`o`, `m`, `f`.", [d for d, _, _ in word[:3]])
    + "\n"
    + ts_string("WORDMARK_PI", "The wordmark's `π`.", f'"{word[3][0]}"')
    + "\n"
    + ts_string("MARK_VIEW_BOX", "`[π]`'s ink box.", box_text(mark_ink))
    + "\n"
    + ts_paths("MARK_BRACKETS", "`[` and `]`.", [mark[0][0], mark[2][0]])
    + "\n"
    + ts_string("MARK_PI", "The mark's `π`.", f'"{mark[1][0]}"')
    + "\n"
    + ts_string("ICON_VIEW_BOX", "The square around `π`, the small icon.", box_text(pi_ink, PAD, square=True))
    + "\n"
    + ts_string("ICON_PI", "The small icon's `π`.", f'"{pi[0][0]}"')
)

# The desktop icon: `[π]` centred in a square window (512 px for icon.png, 192 px for the web app's) with a transparent
# background, brackets in a fixed grey.
mx, my, mw, mh = view_box(mark_ink, square=True)
d0, left, right = mark[1]
icon_body = (
    f'<path d="{mark[0][0]}" fill="{ICON_GREY}"/><path d="{mark[2][0]}" fill="{ICON_GREY}"/>'
    f'<path d="{d0}" fill="url(#omfp-grad)"/>'
)
APP_ICON.write_text(
    f"""<!doctype html>
<!-- The desktop icon (M7, UI-222): the mark `[π]` (UI-220) on a transparent square, the brackets in a grey that reads on
     light and dark launchers, `π` in the logo's fixed gradient. Generated by docs/brand/generate.py and rendered
     headlessly by `just brand` into icon.png (512 px, RGBA, which Tauri needs) and docs/brand/app-icon-192.png (the
     web app's, UI-425): it fills the window, whatever its size. -->
<html>
  <head>
    <meta charset="utf-8" />
    <style>
      html,
      body {{
        margin: 0;
        width: 100vw;
        height: 100vh;
        background: transparent;
        overflow: hidden;
      }}
      svg {{
        display: block;
        width: 87.5vmin;
        height: 87.5vmin;
        margin: 6.25vmin;
      }}
    </style>
  </head>
  <body>
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="{mx} {my} {mw} {mh}">
      <defs>{gradient(left, right)}</defs>
      {icon_body}
    </svg>
  </body>
</html>
"""
)
print("wrote", ", ".join(files), KIT_MODULE.relative_to(ROOT), APP_ICON.relative_to(ROOT))
