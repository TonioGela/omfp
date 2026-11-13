/**
 * The global commands of the frame (UI-095), shared by the keymap and the `:` line: a draft thread in the current
 * workspace and thread actions on the selected thread (UI-021, UI-023, UI-026, UI-030), a rescan of the current
 * workspace (DMN-023), colour scheme (UI-091, window-local), density and `:set` (daemon settings, UI-094, UI-097), the
 * view switch (UI-101), the next and previous view (UI-302) and `:view reset` (UI-107), adding a widget to the view
 * (UI-300), copying a widget into the workspace (UI-106) and saving a workspace widget to the user library (UI-233).
 */

import { pathReference } from "@omfp/kit";
import type { ReactNode } from "react";
import {
  COLOR_SCHEME,
  chooseView,
  currentWorkspaceId,
  FOCUS_COMPOSER,
  type OpenFileOptions,
  openFile,
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
import { DEFAULT_MANIFEST } from "../view/views.ts";
import { displayTitle, isArchived, nextNeedingAttention, spawnerOf } from "../vm/threads.ts";
import { DENSITIES, parseSet, SCHEMES, SET_KEYS } from "./set.ts";

/** The path the focused element or its nearest ancestor carries for `g f` (`data-omfp-path`, UI-116). */
function focusedPath(element: Element | null): { path: string; options: OpenFileOptions } | undefined {
  const holder = element?.closest("[data-omfp-path]");
  const path = holder?.getAttribute("data-omfp-path");
  if (holder === null || holder === undefined || path === null || path === undefined || path === "") {
    return undefined;
  }
  const line = Number(holder.getAttribute("data-omfp-line"));
  return {
    path,
    options: {
      line: Number.isInteger(line) && line > 0 ? line : undefined,
      diff: holder.hasAttribute("data-omfp-diff"),
    },
  };
}

export interface GlobalCommandsProps {
  /** The views of the window, in the order the mode line lists them. */
  readonly viewIds: readonly string[];
  /** The view the window shows. */
  readonly currentView: string | undefined;
  /** What `:widget add` completes (UI-300). */
  readonly widgets: readonly string[];
  /** Adds a widget to the current view (UI-300); resolves to whether `views.json` was written. */
  readonly addWidget: (reference: string) => Promise<boolean>;
}

/** `widget` actions, completed as the first argument of `:widget`. */
const WIDGET_ACTIONS = ["add", "copy", "save", "save!"];

export function GlobalCommands({ viewIds, currentView, widgets, addWidget }: GlobalCommandsProps): ReactNode {
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
    // Without a current workspace, the "+ workspace" field of the left column takes the focus.
    run: () => {
      const workspaceId = currentWorkspaceId(host);
      if (workspaceId === undefined) {
        host.commands.run("workspace.new", [], []);
      } else {
        actions.openDraft(workspaceId);
      }
    },
  });
  useCommand({
    id: "file.open",
    title: titles.open,
    aliases: ["open"],
    enabled: () => currentWorkspaceId(host) !== undefined,
    // `:open <path>[:<line>]`, else the path the focused element (or an ancestor) carries (UI-116).
    run: (args) => {
      const typed = args.length === 0 ? undefined : pathReference(args.join(" "));
      if (typed !== undefined) {
        openFile(host, typed.path, { line: typed.line });
        return;
      }
      const target = focusedPath(document.activeElement);
      if (target !== undefined) {
        openFile(host, target.path, target.options);
      }
    },
  });
  useCommand({
    id: "workspace.rescan",
    title: titles.rescan,
    aliases: ["rescan"],
    enabled: () => currentWorkspaceId(host) !== undefined,
    run: () => {
      const workspaceId = currentWorkspaceId(host);
      if (workspaceId !== undefined) {
        host.client.call("workspaces.rescan", { workspaceId }).catch((error: unknown) => reportError(host, error));
      }
    },
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
    id: "thread.stopSpawned",
    title: titles.stopSpawned,
    aliases: ["stop!"],
    // Offered when the selected thread spawned threads (UI-242).
    enabled: () =>
      selected !== undefined &&
      [...host.frames.getSnapshot().index.threads.values()].some((thread) => spawnerOf(thread) === selected),
    run: onSelected(async (threadId) => {
      try {
        const { threadIds } = await host.client.call("thread.stop", { threadId, cascade: true });
        const thread = summary();
        const title = thread === undefined ? "" : displayTitle(thread);
        host.notices.notify({ title, body: strings.shell.threads.stoppedSpawned(threadIds.length), threadId }, "info", {
          onThread: true,
        });
      } catch (error) {
        reportError(host, error);
      }
    }),
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
        // The jump lands in the thread's composer (UI-286).
        host.window.set(FOCUS_COMPOSER, next);
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
    args: () => [...viewIds, "reset"],
    run: async ([view]) => {
      if (view === "reset") {
        // UI-107: the current default manifest replaces the workspace's views.json, valid or not.
        const workspaceId = currentWorkspaceId(host);
        if (workspaceId === undefined) {
          reportError(host, strings.shell.layout.noWorkspace);
          return;
        }
        try {
          await host.client.call("views.set", { workspaceId, manifest: DEFAULT_MANIFEST });
          host.announcer.announce(strings.shell.layout.reset);
        } catch (error) {
          reportError(host, error);
        }
      } else if (view !== undefined && viewIds.includes(view)) {
        chooseView(host, view);
      } else if (view !== undefined) {
        reportError(host, strings.shell.layout.noView(view));
      }
    },
  });
  // `] v` / `[ v` (UI-302): the next or previous view, round the list.
  const stepView = (step: number) => () => {
    const index = viewIds.indexOf(currentView ?? "");
    const next = viewIds[(index + step + viewIds.length) % viewIds.length];
    if (next !== undefined) {
      chooseView(host, next);
    }
  };
  useCommand({
    id: "view.next",
    title: titles.nextView,
    enabled: () => viewIds.length > 1,
    run: stepView(1),
  });
  useCommand({
    id: "view.prev",
    title: titles.prevView,
    enabled: () => viewIds.length > 1,
    run: stepView(-1),
  });
  // `g v` (UI-302): the `:` line on `view `, whose completions are the views.
  useCommand({
    id: "view.pick",
    title: titles.pickView,
    run: () => {
      host.commands.run("cmdline.open", ["view"]);
    },
  });
  useCommand({
    id: "widget.copy",
    title: titles.widget,
    aliases: ["widget"],
    // `:widget add ` completes the widgets it can add; the other actions take a reference or a name freely.
    args: ([action, ...rest] = []) =>
      action === undefined ? WIDGET_ACTIONS : action === "add" && rest.length === 0 ? widgets : [],
    run: async ([action, widget, name, ...rest]) => {
      const text = strings.shell.widgetCopy;
      if (action === "add" && widget !== undefined && name === undefined) {
        if (await addWidget(widget)) {
          host.announcer.announce(text.added(widget, currentView ?? ""));
        }
        return;
      }
      const known = action === "copy" || action === "save" || action === "save!";
      if (!known || widget === undefined || rest.length > 0) {
        reportError(host, text.usage);
        return;
      }
      const workspaceId = currentWorkspaceId(host);
      if (workspaceId === undefined) {
        reportError(host, strings.shell.layout.noWorkspace);
        return;
      }
      try {
        if (action === "copy") {
          const copied = await host.client.call("widgets.copy", {
            workspaceId,
            widget,
            ...(name === undefined ? {} : { name }),
          });
          host.announcer.announce(text.copied(copied.name));
        } else {
          const saved = await host.client.call("widgets.save", {
            workspaceId,
            widget,
            name: name ?? widget.replace(/^\.\/widgets\//, ""),
            replace: action === "save!",
          });
          host.announcer.announce(text.saved(saved.reference));
        }
      } catch (error) {
        reportError(host, error);
      }
    },
  });
  return null;
}
