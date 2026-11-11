/**
 * The logo (UI-220, UI-221) drawn inline from the generated outlines (logo-paths.ts): the wordmark `omfπ`, the mark
 * `[π]` and the small icon `π`. The text is `currentColor` (`on-surface` with the theme gradient), the brackets the
 * text colour at 62 %, and `π` the theme's gradient (`primary` → `tertiary` → `grad-end`, so user themes and the
 * contrast guard apply), flowing at motion `full`; `gradient="file"` uses the fixed gradient of docs/brand instead.
 */

import { type SVGAttributes, useId } from "react";
import { cx } from "./class.ts";
import {
  ICON_PI,
  ICON_VIEW_BOX,
  MARK_BRACKETS,
  MARK_PI,
  MARK_VIEW_BOX,
  WORDMARK_PI,
  WORDMARK_TEXT,
  WORDMARK_VIEW_BOX,
} from "./logo-paths.ts";
import { useMotionLevel } from "./motion.ts";

/** `theme`: the theme's roles (UI-221); `file`: the fixed gradient of the logo files (UI-220). */
export type LogoGradient = "theme" | "file";

export interface LogoProps extends Omit<SVGAttributes<SVGSVGElement>, "children"> {
  gradient?: LogoGradient;
}

const FILE_STOPS = [
  ["0", "#16b3cc"],
  ["0.62", "#6a5cf0"],
  ["1", "#8a6cf0"],
] as const;

/** `--omfp-grad-loop`'s stops: drawn at twice the `π`'s width, so at rest the first half (`--omfp-grad`) shows. */
const THEME_STOPS = [
  ["0", "primary"],
  ["0.3", "tertiary"],
  ["0.5", "grad-end"],
  ["0.7", "tertiary"],
  ["1", "primary"],
] as const;

/** The brackets' opacity: the text colour, muted. */
const MUTED = "0.62";

function GradientDef({ id, gradient }: { id: string; gradient: LogoGradient }) {
  const motion = useMotionLevel();
  if (gradient === "file") {
    return (
      <linearGradient id={id} x1="0" y1="0" x2="1" y2="0">
        {FILE_STOPS.map(([offset, color]) => (
          <stop key={offset} offset={offset} stopColor={color} />
        ))}
      </linearGradient>
    );
  }
  return (
    <linearGradient id={id} x1="0" y1="0" x2="2" y2="0">
      {THEME_STOPS.map(([offset, role]) => (
        <stop key={offset} offset={offset} style={{ stopColor: `var(--omfp-${role})` }} />
      ))}
      {motion === "full" && (
        // The `omfp-flow` of gradient text (UI-146): 6 s each way, eased, by one width of the `π`.
        <animateTransform
          attributeName="gradientTransform"
          type="translate"
          values="0 0;-1 0;0 0"
          keyTimes="0;0.5;1"
          calcMode="spline"
          keySplines="0.42 0 0.58 1;0.42 0 0.58 1"
          dur="12s"
          repeatCount="indefinite"
        />
      )}
    </linearGradient>
  );
}

/** An SVG of the logo, labelled `omfp` (`aria-hidden` hides it where it repeats a name). */
function Logo({
  viewBox,
  text,
  muted,
  pi,
  gradient = "theme",
  className,
  ...props
}: LogoProps & { viewBox: string; text: readonly string[]; muted: boolean; pi: string }) {
  const id = `omfp-logo-${useId().replace(/[^\w-]/g, "")}`;
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox={viewBox}
      role="img"
      aria-label="omfp"
      {...props}
      className={cx("omfp-logo", className)}
      data-gradient={gradient}
    >
      <defs>
        <GradientDef id={id} gradient={gradient} />
      </defs>
      {text.map((d) => (
        <path key={d} d={d} fill="currentColor" {...(muted ? { opacity: MUTED } : {})} />
      ))}
      <path d={pi} fill={`url(#${id})`} />
    </svg>
  );
}

/** `omfπ`: the wordmark (the brand, UI-170). */
export function Wordmark(props: LogoProps) {
  return <Logo {...props} viewBox={WORDMARK_VIEW_BOX} text={WORDMARK_TEXT} muted={false} pi={WORDMARK_PI} />;
}

/** `[π]`: the mark (the strip's brand, UI-208). */
export function LogoMark(props: LogoProps) {
  return <Logo {...props} viewBox={MARK_VIEW_BOX} text={MARK_BRACKETS} muted pi={MARK_PI} />;
}

/** `π` alone: the icon at 16–22 px (favicon, tray). */
export function LogoIcon(props: LogoProps) {
  return <Logo {...props} viewBox={ICON_VIEW_BOX} text={[]} muted={false} pi={ICON_PI} />;
}
