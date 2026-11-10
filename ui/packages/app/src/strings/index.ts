/**
 * Every text the UI shows, in English (one module, so wording changes and a later translation touch nothing else).
 * Split by area; components import `strings` and never inline user-visible text.
 */

import { chat } from "./chat.ts";
import { shell } from "./shell.ts";

export const strings = { chat, shell } as const;
