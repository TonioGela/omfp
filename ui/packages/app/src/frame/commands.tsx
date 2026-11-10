/**
 * The global commands of the frame (UI-095), shared by the keymap and the `:` line: thread actions on the selected
 * thread (UI-021, UI-023, UI-026, UI-030), colour scheme (UI-091, window-local), density and `:set` (daemon settings,
 * UI-094, UI-097), and the view switch.
 */

import type { ReactNode } from "react";
import {
  ACTIVE_VIEW,
  COLOR_SCHEME,
  type ColorScheme,
  NEW_THREAD_DIALOG,
  RENAMING_THREAD,
  reportError,
  SELECTED_THREAD,
  useCommand,
  useHost,
  useSetSettings,
  useThreadActions,
  useWindowState,
} from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { isArchived, nextNeedingAttention } from "../vm/threads.ts";
import { DENSITIES, parseSet, SET_KEYS } from "./set.ts";

const SCHEMES: readonly ColorScheme[] = ["system", "light", "dark"];

export function GlobalCommands({ viewIds }: { viewIds: readonly string[] }): ReactNode {
  const host = useHost();
  const actions = useThreadActions();
  const setSettings = useSetSettings();
  const [selected] = useWindowState(SELECTED_THREAD);
  const summary = () => (selected === undefined ? undefined : host.frames.getSnapshot().index.threads.get(selected));
  const live = () => {
    const thread = summary();
    return thread !== undefined && !isArchived(thread);
  };
  const archived = () => {
    const thread = summary();
    return thread !== undefined && isArchived(thread);
  };
  const onSelected = (run: (threadId: string) => unknown) => () => {
    if (selected !== undefined) {
      void run(selected);
    }
  };
  const titles = strings.shell.commands;
  useCommand({
    id: "thread.new",
    title: titles.new,
    aliases: ["new"],
    run: () => host.window.set(NEW_THREAD_DIALOG, true),
  });
  useCommand({
    id: "thread.archive",
    title: titles.archive,
    aliases: ["archive"],
    enabled: live,
    run: onSelected(actions.archive),
  });
  useCommand({
    id: "thread.unarchive",
    title: titles.unarchive,
    aliases: ["unarchive"],
    enabled: archived,
    run: onSelected(actions.unarchive),
  });
  useCommand({
    id: "thread.delete",
    title: titles.delete,
    aliases: ["delete"],
    enabled: () => summary() !== undefined,
    run: onSelected(actions.requestDelete),
  });
  useCommand({
    id: "thread.rename",
    title: titles.rename,
    aliases: ["rename"],
    enabled: () => summary() !== undefined,
    run: (args) =>
      onSelected((threadId) =>
        args.length > 0 ? actions.rename(threadId, args.join(" ")) : host.window.set(RENAMING_THREAD, threadId),
      )(),
  });
  useCommand({
    id: "thread.stop",
    title: titles.stop,
    aliases: ["stop"],
    enabled: () => ["running", "starting"].includes(summary()?.process.state ?? ""),
    run: onSelected(actions.stop),
  });
  useCommand({
    id: "thread.restart",
    title: titles.restart,
    aliases: ["restart"],
    enabled: () => ["stopped", "failed"].includes(summary()?.process.state ?? ""),
    run: onSelected(actions.restart),
  });
  useCommand({
    id: "thread.nextAttention",
    title: titles.nextAttention,
    aliases: ["next"],
    run: () => {
      const threads = [...host.frames.getSnapshot().index.threads.values()];
      const next = nextNeedingAttention(threads, selected);
      if (next !== undefined) {
        actions.select(next);
      }
    },
  });
  useCommand({
    id: "theme.set",
    title: titles.theme,
    aliases: ["theme"],
    args: () => SCHEMES,
    run: ([scheme]) => {
      const chosen = SCHEMES.find((candidate) => candidate === scheme);
      if (chosen !== undefined) {
        host.window.set(COLOR_SCHEME, chosen);
      }
    },
  });
  useCommand({
    id: "density.set",
    title: titles.density,
    aliases: ["density"],
    args: () => DENSITIES,
    run: ([density]) => {
      const chosen = DENSITIES.find((candidate) => candidate === density);
      if (chosen !== undefined) {
        void setSettings({ ui: { density: chosen } });
      }
    },
  });
  useCommand({
    id: "settings.set",
    title: titles.set,
    aliases: ["set"],
    args: () => SET_KEYS,
    run: (args) => {
      const parsed = parseSet(args.join(" "));
      if (parsed.ok) {
        void setSettings(parsed.patch);
      } else {
        reportError(host, new Error(parsed.error));
      }
    },
  });
  useCommand({
    id: "view.switch",
    title: titles.view,
    aliases: ["view"],
    args: () => viewIds,
    run: ([view]) => {
      if (view !== undefined && viewIds.includes(view)) {
        host.window.set(ACTIVE_VIEW, view);
      }
    },
  });
  return null;
}
