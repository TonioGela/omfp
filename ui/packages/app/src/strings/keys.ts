/** Texts of the workspace keymap, `.omfp/keymap.json` (UI-210..212). */

const MODES = { normal: "normal", insert: "insert", cmdline: "command line" } as const;

export const keys = {
  /** The banner while the file is refused (UI-212). */
  invalid: (error: string) =>
    `The workspace keymap .omfp/keymap.json is invalid, its last valid bindings stay: ${error}`,
  /** The help panel's group of the workspace's bindings (UI-211). */
  help: {
    section: "workspace",
    /** A workspace binding of another mode than normal: its description and that mode. */
    inMode: (description: string, mode: keyof typeof MODES) => `${description} (${MODES[mode]})`,
  },
} as const;
