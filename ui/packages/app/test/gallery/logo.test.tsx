/** L4: the gallery's logo sheet (UI-173, UI-221): every logo at every size, theme gradient vs the files'. */

import { afterEach, describe, expect, test } from "bun:test";
import { cleanup, render, screen } from "@testing-library/react";
import { LogoSheet } from "../../src/gallery/logo.tsx";

afterEach(cleanup);

describe("logo sheet", () => {
  // spec: UI-173, UI-221, UI-220
  test("wordmark, mark and icon at 48/32/22/16 px: theme gradient on the page, file gradient on light and dark", () => {
    render(<LogoSheet />);
    const tiles = screen.getAllByTestId("logo-tile");
    expect(tiles.map((tile) => [tile.dataset.tile, tile.textContent])).toEqual([
      ["theme", "theme gradient · page surface"],
      ["light", "file gradient · light"],
      ["dark", "file gradient · dark"],
    ]);
    const samples = tiles.map((tile) =>
      [...tile.querySelectorAll("svg.omfp-logo")].map(
        (svg) => `${svg.getAttribute("data-logo")} ${svg.getAttribute("data-gradient")} ${svg.getAttribute("height")}`,
      ),
    );
    const row = (gradient: string) =>
      ["wordmark", "mark", "icon"].flatMap((logo) => [48, 32, 22, 16].map((size) => `${logo} ${gradient} ${size}`));
    expect(samples).toEqual([row("theme"), row("file"), row("file")]);
  });
});
