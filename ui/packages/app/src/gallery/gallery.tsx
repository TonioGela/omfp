/**
 * The `/kit` gallery (UI-173): every kit component of 09-design in every state it lists, side by side, with sizes and
 * both densities, the glyph vocabulary and a token sheet with measured contrast. Hover, pressed and focus are forced
 * with the kit's state classes (`is-hover`, `is-pressed`, `is-focus`), so the page is deterministic; the query picks
 * the scheme, theme, density, motion and root size (params.ts). The screenshot test (UI-174) waits for
 * `[data-ready=true]` and reads the contrast rows.
 */
import "./gallery.css";
import { useEffect, useState } from "react";
import type { Adjustment } from "../theme/contrast.ts";
import { applyContrastGuard, cssColorReader } from "../theme/guard.ts";
import { applyAppearance, applyUserTheme } from "../theme/theme.ts";
import { BorderBoxes, Cards, CodeDemo, Diffs, Interactions, ToolCards } from "./cards.tsx";
import { Composers, LiveVoice, Transcript } from "./chat.tsx";
import { Buttons, Choices, Dialogs, Fields, MenuDemo, PathFields, Selects, TabsDemo } from "./controls.tsx";
import duxTheme from "./dux-solarized.css" with { type: "text" };
import { Empties, Logs, Marks, ProgressDemo, Toasts } from "./feedback.tsx";
import forestTheme from "./matugen-forest.css" with { type: "text" };
import { type GalleryParams, type GalleryTheme, galleryParams } from "./params.ts";
import { HelpPanels, LeftColumn, ModeLines, ResponsiveParts } from "./shell.tsx";
import { DensityStrip, Glyphs, Logo, TokenSheet } from "./tokens.tsx";

/** The fixture themes of `?theme=` (UI-173); Synth is the built-in palette (no user theme). */
const THEME_CSS: Readonly<Record<GalleryTheme, string | null>> = {
  synth: null,
  "matugen-forest": forestTheme as string,
  "dux-solarized": duxTheme as string,
};

/**
 * Puts the query's appearance and theme on the document (the user theme is replaced by the fixture or nothing) and runs
 * the contrast guard (UI-196), before the first render, so what the page measures is in its final colors. Returns
 * what the guard overrode.
 */
function applyGalleryParams(params: GalleryParams): Adjustment[] {
  applyAppearance(document.documentElement, {
    scheme: params.scheme,
    density: params.density,
    fontSize: params.fontSize,
    motion: params.motion,
    fonts: null,
  });
  applyUserTheme(document, THEME_CSS[params.theme]);
  document.title = "omfp kit";
  return applyContrastGuard(document.documentElement, cssColorReader(document));
}

/** Two frames: what the last state change shows has been laid out and painted. */
function afterPaint(): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  return promise;
}

/** The `/kit` page. */
export function Gallery({ search }: { search: string }) {
  const [{ params, guard }] = useState(() => {
    const parsed = galleryParams(search);
    return { params: parsed, guard: applyGalleryParams(parsed) };
  });
  const [measured, setMeasured] = useState(false);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    if (!measured) {
      return;
    }
    let live = true;
    void (async () => {
      await document.fonts.ready;
      await afterPaint();
      await document.fonts.ready;
      if (live) {
        setReady(true);
      }
    })();
    return () => {
      live = false;
    };
  }, [measured]);
  return (
    <main className="omfp-gallery" data-testid="kit-gallery" data-ready={ready}>
      <header className="omfp-gallery__top">
        <h1 className="omfp-gallery__page-title omfp-grad-text omfp-grad-flow">◆ omfp kit</h1>
        <span className="omfp-gallery__note">
          Crush · {params.theme} · {params.scheme} · {params.density} · motion {params.motion} · {params.fontSize}px
        </span>
      </header>
      <div className="omfp-gallery__sections">
        <Buttons />
        <Logo />
        <Glyphs />
        <Fields />
        <PathFields />
        <Selects />
        <Choices />
        <TabsDemo />
        <MenuDemo />
        <Marks />
        <Dialogs />
        <ProgressDemo />
        <Logs />
        <Toasts />
        <ModeLines />
        <HelpPanels />
        <Empties />
        <Cards />
        <ToolCards />
        <BorderBoxes />
        <Interactions />
        <Diffs />
        <CodeDemo />
        <LeftColumn />
        <ResponsiveParts />
        <Transcript />
        <Composers />
        <LiveVoice />
        <DensityStrip />
        <TokenSheet guard={guard} onMeasured={() => setMeasured(true)} />
      </div>
    </main>
  );
}
