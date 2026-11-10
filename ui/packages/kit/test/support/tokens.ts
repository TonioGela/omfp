/** The token names of the M3 kit contract (without the `--omfp-` prefix). */

const ROLE_GROUPS = ["primary", "secondary", "tertiary", "error"] as const;

export const COLOR_TOKENS: readonly string[] = [
  ...ROLE_GROUPS.flatMap((role) => [role, `on-${role}`, `${role}-container`, `on-${role}-container`]),
  "surface",
  "on-surface",
  "surface-variant",
  "on-surface-variant",
  "surface-container-lowest",
  "surface-container-low",
  "surface-container",
  "surface-container-high",
  "surface-container-highest",
  "surface-dim",
  "surface-bright",
  "inverse-surface",
  "inverse-on-surface",
  "inverse-primary",
  "outline",
  "outline-variant",
  "shadow",
  "scrim",
  "success",
  "on-success",
  "warning",
  "on-warning",
];

export const FONT_TOKENS: readonly string[] = ["font-prose", "font-ui", "font-code"];

export const DENSITY_TOKENS: readonly string[] = [
  "font-size-sm",
  "font-size-md",
  "font-size-lg",
  "space-1",
  "space-2",
  "space-3",
  "space-4",
  "space-5",
  "space-6",
  "row-height",
];

export const SHAPE_TOKENS: readonly string[] = ["radius-sm", "radius-md", "radius-lg"];

/** Text color over background color pairs that must reach WCAG AA (4.5:1) in every scheme. */
export const TEXT_PAIRS: readonly (readonly [text: string, background: string])[] = [
  ...ROLE_GROUPS.flatMap((role) => [
    [`on-${role}`, role] as const,
    [`on-${role}-container`, `${role}-container`] as const,
  ]),
  ["on-success", "success"],
  ["on-warning", "warning"],
  ["on-surface", "surface"],
  ["on-surface", "surface-container-lowest"],
  ["on-surface", "surface-container-low"],
  ["on-surface", "surface-container"],
  ["on-surface", "surface-container-high"],
  ["on-surface", "surface-container-highest"],
  ["on-surface-variant", "surface"],
  ["on-surface-variant", "surface-variant"],
  ["on-surface-variant", "surface-container-highest"],
  ["inverse-on-surface", "inverse-surface"],
  ["primary", "surface"],
  ["error", "surface"],
  ["success", "surface"],
  ["warning", "surface"],
];

/** The token name as a custom property. */
export function prop(token: string): string {
  return `--omfp-${token}`;
}
