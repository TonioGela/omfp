/**
 * The gallery's logo sheet (UI-173): the wordmark, the mark and the small icon (UI-220) at 48, 32, 22 and 16 px tall,
 * with the theme's gradient on the page's surface (UI-221, the scheme of `?scheme=`), then with the fixed gradient of
 * the docs/brand files on a light and a dark tile in the files' text colours.
 */
import { type LogoGradient, LogoIcon, LogoMark, type LogoProps, Wordmark } from "@omfp/kit";
import type { ComponentType } from "react";

const LOGOS: readonly (readonly [name: string, Logo: ComponentType<LogoProps>])[] = [
  ["wordmark", Wordmark],
  ["mark", LogoMark],
  ["icon", LogoIcon],
];

const SIZES = [48, 32, 22, 16] as const;

const TILES: readonly { tile: string; gradient: LogoGradient; label: string }[] = [
  { tile: "theme", gradient: "theme", label: "theme gradient · page surface" },
  { tile: "light", gradient: "file", label: "file gradient · light" },
  { tile: "dark", gradient: "file", label: "file gradient · dark" },
];

export function LogoSheet() {
  return (
    <>
      {TILES.map(({ tile, gradient, label }) => (
        <div key={tile} className="omfp-gallery__logo-tile" data-tile={tile} data-testid="logo-tile">
          <span className="omfp-gallery__label omfp-gallery__logo-label">{label}</span>
          {LOGOS.map(([name, Logo]) => (
            <div key={name} className="omfp-gallery__logo-sizes">
              {SIZES.map((size) => (
                <Logo key={size} gradient={gradient} height={size} data-logo={name} />
              ))}
            </div>
          ))}
        </div>
      ))}
    </>
  );
}
