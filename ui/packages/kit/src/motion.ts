/** The motion levels of `[ui] motion` (UI-145). */
export type MotionLevel = "full" | "reduced" | "none";

const LEVELS: readonly string[] = ["full", "reduced", "none"];

/**
 * The effective motion level for script-driven effects (count-up, smooth scroll, UI-145): `<html data-motion>`, which
 * the app sets to the setting capped by the system; without it, `none` while the system asks for reduced motion, else
 * `full`.
 */
export function motionLevel(root: HTMLElement = document.documentElement): MotionLevel {
  const level = root.dataset.motion;
  if (level !== undefined && LEVELS.includes(level)) {
    return level as MotionLevel;
  }
  const reduce =
    typeof window.matchMedia === "function" && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  return reduce ? "none" : "full";
}
