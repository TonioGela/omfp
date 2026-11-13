/**
 * Small status marks (UI-157, UI-158, UI-165): state glyphs and badges, counts, exit codes, repository kinds, diff
 * stats and the token meter. Glyphs are text in the code family (UI-122), drawn by kit.css from `data-state`, so the
 * running spinner can cycle its braille frames at motion `full` and `reduced` and rest on its first frame at `none`.
 */
import { type HTMLAttributes, type ReactNode, useEffect, useState } from "react";
import { Meter as AriaMeter } from "react-aria-components";
import { cx } from "./class.ts";
import { Icon, type IconName } from "./icons.tsx";
import { motionLevel } from "./motion.ts";

/** A thread's state (UI-137). */
export type ThreadState = "running" | "awaiting" | "failed" | "unread" | "idle";

/** What a state mark shows: a thread state, or a finished tool call (`done`; a failed call is `failed`). */
export type MarkState = ThreadState | "done";

export interface StateMarkProps extends Omit<HTMLAttributes<HTMLSpanElement>, "children"> {
  state: MarkState;
  /** Accessible name; without it the mark is decorative (the text next to it says the state). */
  label?: string;
  /**
   * The running spinner (UI-216): `activity` (`⠋…⠏`, threads and the header) or omp's `status` (`⣾…⣷`, tool cards).
   */
  spinner?: "activity" | "status";
}

/** The glyph of a state (UI-122) in its status color (UI-137), in an icon-sized cell. */
export function StateMark({ state, label, spinner = "activity", className, ...props }: StateMarkProps) {
  return (
    <span
      {...(label === undefined ? { "aria-hidden": true } : { role: "img", "aria-label": label })}
      {...props}
      className={cx("omfp-state-mark", className)}
      data-state={state}
      data-spinner={spinner === "status" ? spinner : undefined}
    />
  );
}

export interface StateBadgeProps extends HTMLAttributes<HTMLSpanElement> {
  state: ThreadState;
  /** The word shown after the glyph (the state's name by default). */
  children?: ReactNode;
  /** Just changed: pops once at motion `full` (UI-146). */
  fresh?: boolean;
  /** No box: glyph and word only, where a box would read as a control among buttons (the chat header, UI-483). */
  bare?: boolean;
}

/** A thread or tool state as a bordered badge: glyph + word (UI-157); the word in its own span, which a phone's chat
 * header hides visually (UI-419). */
export function StateBadge({ state, fresh = false, bare = false, className, children, ...props }: StateBadgeProps) {
  return (
    <span
      {...props}
      className={cx("omfp-state-badge", className)}
      data-state={state}
      data-fresh={fresh || undefined}
      data-bare={bare || undefined}
    >
      <StateMark state={state} />
      <span className="omfp-state-badge__word">{children ?? state}</span>
    </span>
  );
}

export type CountProps = HTMLAttributes<HTMLSpanElement>;

/** A neutral count pill (UI-157). */
export function Count({ className, ...props }: CountProps) {
  return <span {...props} className={cx("omfp-count", className)} />;
}

export interface ExitCodeProps extends Omit<HTMLAttributes<HTMLSpanElement>, "children"> {
  code: number;
}

/** A process exit code: `exit 0` in ok, anything else in error (UI-137). */
export function ExitCode({ code, className, ...props }: ExitCodeProps) {
  return (
    <span {...props} className={cx("omfp-exit", className)} data-ok={code === 0}>
      exit {code}
    </span>
  );
}

/** How a repository sits in a workspace folder (UI-157). */
export type RepoKind = "worktree" | "clone" | "link";

const KIND_ICON: Record<RepoKind, IconName> = { worktree: "git-fork", clone: "copy", link: "link-2" };

export interface KindMarkProps extends HTMLAttributes<HTMLSpanElement> {
  kind: RepoKind;
  /** The kind's word (the kind's name by default). */
  children?: ReactNode;
}

/** A repository kind: its icon and word in the kind's color (UI-157). */
export function KindMark({ kind, className, children, ...props }: KindMarkProps) {
  return (
    <span {...props} className={cx("omfp-kind", className)} data-kind={kind}>
      <Icon name={KIND_ICON[kind]} />
      {children ?? kind}
    </span>
  );
}

/** Duration of the count-up (UI-146). */
const COUNT_UP_MS = 700;

/**
 * `to`, counted up from 0 over `ms` when the effective motion level is `full` (UI-146), else `to` at once. The value
 * eases out; it reaches `to` exactly at the end.
 */
export function useCountUp(to: number, ms: number = COUNT_UP_MS): number {
  const [shown, setShown] = useState(() => (motionLevel() === "full" ? 0 : to));
  useEffect(() => {
    if (motionLevel() !== "full") {
      setShown(to);
      return undefined;
    }
    let frame = 0;
    let start: number | undefined;
    const tick = (now: number) => {
      start ??= now;
      const t = Math.min(1, (now - start) / ms);
      setShown(t >= 1 ? to : Math.round(to * (1 - (1 - t) ** 3)));
      if (t < 1) {
        frame = requestAnimationFrame(tick);
      }
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [to, ms]);
  return shown;
}

export interface DiffStatProps extends Omit<HTMLAttributes<HTMLSpanElement>, "children"> {
  added: number;
  removed: number;
}

/** `+n −m` in ok and error, counting up when first shown at motion `full` (UI-165); the space reads in its text. */
export function DiffStat({ added, removed, className, ...props }: DiffStatProps) {
  const shownAdded = useCountUp(added);
  const shownRemoved = useCountUp(removed);
  return (
    <span {...props} className={cx("omfp-diffstat", className)}>
      <span className="omfp-diffstat__add">+{shownAdded}</span>{" "}
      <span className="omfp-diffstat__del">−{shownRemoved}</span>
    </span>
  );
}

/** `48200` → `48.2k`, `1500000` → `1.5M`: token counts in the mode line. */
export function compactNumber(value: number): string {
  const abs = Math.abs(value);
  const [divisor, suffix] = abs >= 1e6 ? [1e6, "M"] : abs >= 1e3 ? [1e3, "k"] : [1, ""];
  const scaled = value / divisor;
  const text = divisor === 1 || Math.abs(scaled) >= 100 ? String(Math.round(scaled)) : scaled.toFixed(1);
  return `${text.replace(/\.0$/, "")}${suffix}`;
}

export interface MeterProps {
  value: number;
  max: number;
  /** Accessible name. */
  label: string;
  /** Fraction of `max` above which the meter turns warn (UI-158); 0.8 by default. */
  threshold?: number;
  className?: string;
  "data-testid"?: string;
}

/** A small usage bar with `used/total` beside it (the token meter, UI-158). */
export function Meter({ value, max, label, threshold = 0.8, className, "data-testid": testId }: MeterProps) {
  const high = value > max * threshold;
  return (
    <AriaMeter
      value={value}
      maxValue={max}
      aria-label={label}
      className={cx("omfp-gauge", className)}
      data-high={high || undefined}
      {...(testId === undefined ? {} : { "data-testid": testId })}
    >
      {({ percentage }) => (
        <>
          <span className="omfp-gauge__bar">
            <span className="omfp-gauge__fill" style={{ inlineSize: `${percentage}%` }} />
          </span>
          <span>
            {compactNumber(value)}/{compactNumber(max)}
          </span>
        </>
      )}
    </AriaMeter>
  );
}
