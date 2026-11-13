/**
 * Init files (UI-260..UI-263, experimental, branch idea/init): the user's `init.ts` (next to `theme.css`, served by
 * the daemon) and the current workspace's `.omfp/init.ts`, ES modules compiled like widgets. Their default export is
 * called with an {@link InitApi} (the user's file first, then the workspace's); what it registers through the API —
 * commands, key bindings, views, floating panels, thread event listeners, cleanups — is removed again, and the function
 * it returned is called, before the file runs again (a new version, or another current workspace) or goes away. A
 * failure anywhere (compile error, import, the run, a cleanup, a command or listener it registered) becomes the file's
 * error, shown in a banner by the frame; the window keeps working.
 */

import type { ThreadSummary } from "@omfp/protocol";
import {
  type InitBindings,
  type KeymapOverride,
  type Mode,
  NO_INIT_BINDINGS,
  normalizeKeys,
} from "../keymap/keymap.ts";
import { strings } from "../strings/index.ts";
import { type LayoutNode, parseLayout } from "../view/layout.ts";
import type { ResolvedPanel, ResolvedView } from "../view/views.ts";
import { errorMessage } from "./actions.ts";
import { type Command, scopesAt } from "./commands.ts";
import type { OmfpHost, SdkClient } from "./host.tsx";
import { classifyModule, type Importer } from "./modules.ts";
import type { NoticeRequest } from "./notify.ts";

const text = strings.init;

/** Which init file: the user's (`~/.config/omfp/init.ts`) or the current workspace's (`.omfp/init.ts`). */
export type InitFile = "user" | "workspace";

/** The thread changes an init file can listen to (the same as the window's notices, UI-192). */
export type ThreadEventKind = "attention" | "failed" | "finished";

/** A key binding an init file makes; `mode` defaults to `normal`. */
export interface InitBinding {
  readonly mode?: Mode;
  readonly keys: string;
  /** A command id; `""` removes the binding of `keys` in `mode`. */
  readonly command: string;
  readonly args?: readonly string[];
}

/** A command an init file registers: a {@link Command} without `init`, which the API sets. */
export type InitCommand = Omit<Command, "init">;

/** A view an init file adds to the view switcher of this window. */
export interface InitView {
  /** Default: the id. */
  readonly title?: string;
  /** A layout node, as in `views.json`. */
  readonly layout: unknown;
}

/** What an init file's default export receives. */
export interface InitApi {
  /** Which file this is. */
  readonly file: InitFile;
  /** The workspace whose `.omfp/init.ts` this is; `undefined` for the user's file. */
  readonly workspaceId: string | undefined;
  /** The window's host (registry, window store, notices, protocol client): everything else the SDK does. */
  readonly host: OmfpHost;
  /** Every protocol method, typed: `call("thread.prompt", { threadId, text })`. */
  readonly call: SdkClient["call"];
  /** Registers a command for the keymap and the `:` line; returns its removal. */
  command(command: InitCommand): () => void;
  /** Binds keys over the defaults, `[keymap]` and `.omfp/keymap.json`; returns the removal of these bindings. */
  bind(bindings: InitBinding | readonly InitBinding[]): () => void;
  /** A notice in the mode line, or a system notification while the window has no focus (UI-194). */
  notify(request: NoticeRequest): void;
  /** Calls `listener` whenever a thread starts needing the user, fails, or finishes a turn; returns its removal. */
  on(event: ThreadEventKind, listener: (thread: ThreadSummary) => void): () => void;
  /** Adds (or, with the id of a manifest view, replaces) a view in this window's view switcher. */
  view(id: string, view: InitView): () => void;
  /** Floats `node` (a layout node; its `title` prop labels it) over every view of this window. */
  panel(id: string, node: unknown): () => void;
  /** Runs a command by id or alias in the scopes around the focused element; `false` when nothing answers. */
  run(name: string, args?: readonly string[]): boolean;
  /** Calls `cleanup` before the file runs again or goes away. */
  onCleanup(cleanup: () => void): void;
}

/** Why an init file failed. */
export interface InitError {
  readonly file: InitFile;
  readonly message: string;
}

/** What the init files currently add to the window. */
export interface InitSnapshot {
  readonly bindings: InitBindings;
  /** The user's views, then the workspace's, each in registration order. */
  readonly views: readonly ResolvedView[];
  readonly panels: readonly ResolvedPanel[];
  readonly errors: readonly InitError[];
}

const EMPTY: InitSnapshot = { bindings: NO_INIT_BINDINGS, views: [], panels: [], errors: [] };

interface Listener {
  readonly event: ThreadEventKind;
  readonly listener: (thread: ThreadSummary) => void;
}

/** One run of an init file, from its import until it is replaced. */
interface Run {
  readonly file: InitFile;
  readonly url: string | undefined;
  readonly workspaceId: string | undefined;
  /** `false` once replaced: what it still registers is removed at once. */
  live: boolean;
  /** Removals and cleanups, called newest first. */
  readonly disposers: Array<() => void>;
  bindings: readonly KeymapOverride[];
  readonly views: Map<string, ResolvedView>;
  readonly panels: Map<string, ResolvedPanel>;
  readonly listeners: Set<Listener>;
}

function newRun(file: InitFile, url: string | undefined, workspaceId: string | undefined): Run {
  return {
    file,
    url,
    workspaceId,
    live: true,
    disposers: [],
    bindings: [],
    views: new Map(),
    panels: new Map(),
    listeners: new Set(),
  };
}

const FILES: readonly InitFile[] = ["user", "workspace"];

/** The window's init files: their runs and what they add (the frame reads {@link InitFiles.getSnapshot}). */
export class InitFiles {
  readonly #host: () => OmfpHost;
  readonly #importer: Importer;
  readonly #runs: Record<InitFile, Run> = {
    user: newRun("user", undefined, undefined),
    workspace: newRun("workspace", undefined, undefined),
  };
  readonly #errors: Record<InitFile, string | undefined> = { user: undefined, workspace: undefined };
  #queue: Promise<void> = Promise.resolve();
  #snapshot: InitSnapshot = EMPTY;
  readonly #listeners = new Set<() => void>();

  /** `host` is read lazily: the host holds this object. */
  constructor(host: () => OmfpHost, importer: Importer) {
    this.#host = host;
    this.#importer = importer;
  }

  readonly getSnapshot = (): InitSnapshot => this.#snapshot;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  };

  /**
   * Makes `url` (`undefined`: none) the module of `file`, for `workspaceId` (the workspace's file): unless it already
   * is, the current run is cleaned up and the new module imported and run. Loads run one at a time, in call order.
   */
  load(file: InitFile, url: string | undefined, workspaceId?: string): Promise<void> {
    const next = this.#queue.then(() => this.#load(file, url, workspaceId));
    this.#queue = next;
    return next;
  }

  /** Resolves once the loads asked for so far have run (tests wait on it). */
  settled(): Promise<void> {
    return this.#queue;
  }

  /** Every listener of `event`, the user's file's first (the frame calls it for each thread change). */
  emit(event: ThreadEventKind, thread: ThreadSummary): void {
    for (const file of FILES) {
      const run = this.#runs[file];
      for (const entry of [...run.listeners]) {
        if (entry.event === event) {
          this.#guard(
            run,
            (message) => text.listener(event, message),
            () => entry.listener(thread),
          );
        }
      }
    }
  }

  /** Closes a floating panel an init file added, in this window until the file runs again. */
  closePanel(id: string): void {
    for (const file of FILES) {
      if (this.#runs[file].panels.delete(id)) {
        this.#changed();
      }
    }
  }

  async #load(file: InitFile, url: string | undefined, workspaceId: string | undefined): Promise<void> {
    const current = this.#runs[file];
    if (current.url === url && current.workspaceId === workspaceId) {
      return;
    }
    this.#errors[file] = undefined;
    this.#dispose(current);
    const run = newRun(file, url, workspaceId);
    this.#runs[file] = run;
    this.#changed();
    if (url === undefined) {
      return;
    }
    let module: unknown;
    try {
      module = await this.#importer(url);
    } catch (error) {
      this.#fail(run, text.load(errorMessage(error)));
      return;
    }
    const loaded = classifyModule(module, text.noDefaultExport);
    if (loaded.kind === "error") {
      const { file: name, line, column, message } = loaded.error;
      const at = line === null ? "" : `:${line}${column === null ? "" : `:${column}`}`;
      this.#fail(run, name === undefined ? message : `${name}${at}: ${message}`);
      return;
    }
    const main = loaded.component as unknown as (api: InitApi) => unknown;
    this.#guard(
      run,
      (message) => message,
      () => main(this.#api(run)),
      (cleanup) => {
        if (typeof cleanup === "function") {
          this.#own(run, cleanup as () => void);
        }
      },
    );
  }

  /** Runs `work`, turning a throw or a rejected promise into `run`'s error; `done` gets what it returned or resolved. */
  #guard(run: Run, describe: (message: string) => string, work: () => unknown, done?: (value: unknown) => void): void {
    const fail = (error: unknown) => this.#fail(run, describe(errorMessage(error)));
    try {
      const result = work();
      if (result instanceof Promise) {
        result.then((value: unknown) => done?.(value), fail);
      } else {
        done?.(result);
      }
    } catch (error) {
      fail(error);
    }
  }

  /** Keeps `dispose` for `run`'s cleanup and returns it callable once; called at once when `run` is gone already. */
  #own(run: Run, dispose: () => void): () => void {
    if (!run.live) {
      this.#guard(run, text.cleanup, dispose);
      return () => {};
    }
    let disposed = false;
    const once = () => {
      if (!disposed) {
        disposed = true;
        const index = run.disposers.indexOf(once);
        if (index >= 0) {
          run.disposers.splice(index, 1);
        }
        dispose();
      }
    };
    run.disposers.push(once);
    return once;
  }

  #dispose(run: Run): void {
    run.live = false;
    for (const dispose of [...run.disposers].reverse()) {
      try {
        dispose();
      } catch (error) {
        this.#errors[run.file] = text.cleanup(errorMessage(error));
      }
    }
  }

  #fail(run: Run, message: string): void {
    if (run.live) {
      this.#errors[run.file] = message;
      this.#changed();
    }
  }

  #api(run: Run): InitApi {
    const host = this.#host();
    const changedIn = (change: () => void) => () => {
      change();
      this.#changed();
    };
    return {
      file: run.file,
      workspaceId: run.workspaceId,
      host,
      call: host.client.call.bind(host.client),
      command: (command) =>
        this.#own(
          run,
          host.commands.register({
            ...command,
            init: run.file,
            run: (args) =>
              this.#guard(
                run,
                (message) => text.command(command.id, message),
                () => command.run(args),
              ),
          }),
        ),
      bind: (bindings) => {
        const entries: KeymapOverride[] = (Array.isArray(bindings) ? bindings : [bindings]).map(
          (binding: InitBinding) => ({
            mode: binding.mode ?? "normal",
            keys: normalizeKeys(binding.keys),
            command: binding.command,
            ...(binding.args === undefined ? {} : { args: binding.args }),
          }),
        );
        run.bindings = [...run.bindings, ...entries];
        this.#changed();
        return this.#own(
          run,
          changedIn(() => {
            run.bindings = run.bindings.filter((binding) => !entries.includes(binding));
          }),
        );
      },
      notify: (request) => host.notices.notify(request),
      on: (event, listener) => {
        const entry: Listener = { event, listener };
        run.listeners.add(entry);
        return this.#own(run, () => run.listeners.delete(entry));
      },
      view: (id, view) => {
        const parsed = parseLayout(view.layout, `views.${id}.layout`);
        if (!parsed.ok) {
          throw new Error(text.invalid(parsed.error));
        }
        // Checked by `parseLayout` just above.
        const layout = view.layout as LayoutNode;
        const resolved: ResolvedView = { id, title: view.title ?? id, layout, tree: parsed.value, init: run.file };
        run.views.set(id, resolved);
        this.#changed();
        return this.#own(
          run,
          changedIn(() => {
            if (run.views.get(id) === resolved) {
              run.views.delete(id);
            }
          }),
        );
      },
      panel: (id, node) => {
        const parsed = parseLayout(node, `panels.${id}`);
        if (!parsed.ok) {
          throw new Error(text.invalid(parsed.error));
        }
        const { title } = parsed.value.props;
        const resolved: ResolvedPanel = {
          id,
          title: typeof title === "string" ? title : id,
          // Checked by `parseLayout` just above.
          node: node as LayoutNode,
          tree: parsed.value,
          init: run.file,
        };
        run.panels.set(id, resolved);
        this.#changed();
        return this.#own(
          run,
          changedIn(() => {
            if (run.panels.get(id) === resolved) {
              run.panels.delete(id);
            }
          }),
        );
      },
      run: (name, args = []) => host.commands.run(name, args, scopesAt(document.activeElement)),
      onCleanup: (cleanup) => {
        this.#own(run, cleanup);
      },
    };
  }

  #changed(): void {
    const { user, workspace } = this.#runs;
    this.#snapshot = {
      bindings: { user: user.bindings, workspace: workspace.bindings },
      views: [...user.views.values(), ...workspace.views.values()],
      panels: [...user.panels.values(), ...workspace.panels.values()],
      errors: FILES.flatMap((file) => {
        const message = this.#errors[file];
        return message === undefined ? [] : [{ file, message }];
      }),
    };
    for (const listener of [...this.#listeners]) {
      listener();
    }
  }
}
