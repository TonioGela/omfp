import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import {
  bandEdges,
  type Levels,
  LivePanel,
  meterValue,
  SPECTRUM_BARS,
  Spectrum,
  type SpectrumSource,
  spectrumFrame,
} from "../src/index.ts";

afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.motion;
});

/** A source with fixed bars and levels, counting its reads. */
class FixedSource implements SpectrumSource {
  reads = 0;
  constructor(
    readonly bars: { mic: number; remote: number },
    readonly rms: Levels,
  ) {}

  read(mic: Float32Array, remote: Float32Array): Levels {
    this.reads++;
    mic.fill(this.bars.mic);
    remote.fill(this.bars.remote);
    return this.rms;
  }

  levels(): Levels {
    return this.rms;
  }
}

function frame(status: Parameters<typeof spectrumFrame>[0], mic: number, remote: number, now = 0) {
  const heights = new Float32Array(SPECTRUM_BARS);
  const fromRemote = new Uint8Array(SPECTRUM_BARS);
  spectrumFrame(
    status,
    new Float32Array(SPECTRUM_BARS).fill(mic),
    new Float32Array(SPECTRUM_BARS).fill(remote),
    now,
    100,
    heights,
    fromRemote,
  );
  return { heights: [...heights], fromRemote: [...fromRemote] };
}

describe("spectrum bars", () => {
  // spec: UI-182
  test("64 log-spaced bands over the voice range, each at least one FFT bin wide", () => {
    const edges = new Uint16Array(SPECTRUM_BARS + 1);
    // 48 kHz, fftSize 1024: 512 bins of 46.875 Hz.
    bandEdges(46.875, 512, edges);
    expect(edges[0]).toBe(1);
    expect(edges[SPECTRUM_BARS]).toBe(192);
    for (let edge = 1; edge <= SPECTRUM_BARS; edge++) {
      expect(edges[edge] ?? 0).toBeGreaterThan(edges[edge - 1] ?? 0);
    }
  });

  // spec: UI-182, UI-183
  test("the louder voice wins each bar; a muted microphone draws nothing; idle and error stay flat", () => {
    const listening = frame("listening", 0.5, 0.2);
    expect(listening.heights.every((height) => height === 50)).toBe(true);
    expect(listening.fromRemote.every((remote) => remote === 0)).toBe(true);
    const speaking = frame("speaking", 0.2, 0.8);
    expect(speaking.heights.every((height) => Math.abs(height - 80) < 1e-4)).toBe(true);
    expect(speaking.fromRemote.every((remote) => remote === 1)).toBe(true);
    // The floor is 6 % of the height.
    expect(frame("muted", 0.9, 0).heights.every((height) => height === 6)).toBe(true);
    expect(frame("idle", 0.9, 0.9).heights.every((height) => height === 6)).toBe(true);
    expect(frame("error", 0.9, 0.9).heights.every((height) => height === 6)).toBe(true);
  });

  // spec: UI-182
  test("connecting sweeps a low wave whatever the audio; working breathes under the voice", () => {
    const connecting = frame("connecting", 1, 1, 1000);
    expect(Math.max(...connecting.heights)).toBeLessThanOrEqual(8);
    expect(new Set(connecting.heights).size).toBeGreaterThan(1);
    expect(Math.min(...frame("working", 0, 0, 0).heights)).toBeGreaterThanOrEqual(6);
  });

  // spec: UI-182
  test("the meter shows the louder voice on a square-root scale, nothing from a muted microphone", () => {
    expect(meterValue("listening", { mic: 0.04, remote: 0 })).toBe(32);
    expect(meterValue("speaking", { mic: 0, remote: 1 })).toBe(100);
    expect(meterValue("muted", { mic: 0.5, remote: 0 })).toBe(0);
    expect(meterValue("idle", { mic: 0.5, remote: 0.5 })).toBe(0);
  });
});

describe("Spectrum", () => {
  // spec: UI-182, UI-145
  test("below motion full it is a static meter, at full a canvas that reads the source every frame", async () => {
    const source = new FixedSource({ mic: 0.5, remote: 0 }, { mic: 0.04, remote: 0 });
    document.documentElement.dataset.motion = "reduced";
    render(<Spectrum source={source} status="listening" label="Voice level" data-testid="spectrum" />);
    const meter = screen.getByRole("meter", { name: "Voice level" });
    expect(meter.getAttribute("aria-valuenow")).toBe("32");
    expect(meter.dataset.voice).toBe("user");
    expect(screen.getByTestId("spectrum").querySelector("canvas")).toBeNull();

    act(() => {
      document.documentElement.dataset.motion = "full";
    });
    await waitFor(() => expect(screen.getByTestId("spectrum").querySelector("canvas")).not.toBeNull());
    expect(screen.queryByRole("meter")).toBeNull();
    await waitFor(() => expect(source.reads).toBeGreaterThan(1));

    act(() => {
      document.documentElement.dataset.motion = "none";
    });
    await waitFor(() => expect(screen.getByRole("meter").getAttribute("aria-valuenow")).toBe("32"));
  });

  // spec: UI-184
  test("the canvas reports the levels of every frame", async () => {
    document.documentElement.dataset.motion = "full";
    const source = new FixedSource({ mic: 0, remote: 0.6 }, { mic: 0, remote: 0.2 });
    const seen: Levels[] = [];
    render(<Spectrum source={source} status="speaking" label="Voice level" onLevels={(levels) => seen.push(levels)} />);
    await waitFor(() => expect(seen.length).toBeGreaterThan(1));
    expect(seen[0]).toEqual({ mic: 0, remote: 0.2 });
  });

  // spec: UI-182
  test("an explicit motion level overrides the effective one", () => {
    document.documentElement.dataset.motion = "none";
    const source = new FixedSource({ mic: 0, remote: 0 }, { mic: 0, remote: 0 });
    render(<Spectrum source={source} status="idle" label="Voice level" motion="full" data-testid="spectrum" />);
    expect(screen.getByTestId("spectrum").querySelector("canvas")).not.toBeNull();
  });
});

describe("LivePanel", () => {
  // spec: UI-182, UI-187
  test("title on the border, spectrum, the latest line per role, then the status line with its key chips", () => {
    render(
      <LivePanel
        title="Live voice"
        status="listening"
        statusLabel="listening"
        spectrum={<div data-testid="bars" />}
        lines={[
          { role: "user", text: "Ciao, mi senti?" },
          { role: "assistant", text: "Sì." },
        ]}
        hints={[
          { keys: "spc", label: "mute" },
          { keys: "esc", label: "end" },
        ]}
        data-testid="live"
      />,
    );
    const panel = screen.getByTestId("live");
    expect(panel.dataset.status).toBe("listening");
    expect(panel.querySelector(".omfp-bbox__head .omfp-bbox__label")?.textContent).toBe("Live voice");
    expect(screen.getByTestId("bars")).toBeDefined();
    expect([...panel.querySelectorAll(".omfp-live__line")].map((line) => line.getAttribute("data-role"))).toEqual([
      "user",
      "assistant",
    ]);
    expect(panel.querySelector(".omfp-live__status")?.textContent).toBe("●listening·spc mute·esc end");
    expect([...panel.querySelectorAll("kbd")].map((kbd) => kbd.textContent)).toEqual(["spc", "esc"]);
  });

  // spec: UI-182, UI-186
  test("a failure replaces the lines, as an alert", () => {
    render(
      <LivePanel
        title="Live voice"
        status="error"
        statusLabel="error"
        lines={[{ role: "user", text: "hidden" }]}
        message="live voice needs an `openai-codex` login in omp"
        note="ended"
        actions={<button type="button">Dismiss</button>}
        data-testid="live"
      />,
    );
    expect(screen.getByRole("alert").textContent).toBe("live voice needs an `openai-codex` login in omp");
    expect(screen.getByTestId("live").querySelector(".omfp-live__line")).toBeNull();
    expect(screen.getByTestId("live").querySelector(".omfp-live__note")?.textContent).toBe("ended");
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeDefined();
  });
});
