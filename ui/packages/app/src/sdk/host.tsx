/**
 * The host every widget runs in: the protocol client, the per-frame model snapshots, window-local state, the command
 * registry, the live region and the widget modules. The frame creates one per window and provides it with {@link OmfpProvider}; widgets
 * reach it only through the hooks of `./hooks.ts`.
 */

import type { CallMethod } from "@omfp/client";
import type { ParamsOf, ResultOf, Topic } from "@omfp/protocol";
import { createContext, type ReactNode, useContext } from "react";
import { strings } from "../strings/index.ts";
import { CommandRegistry } from "./commands.ts";
import { type FrameScheduler, FrameStore, type ModelSource } from "./frame-store.ts";
import { browserImporter, type Importer, WidgetModules } from "./modules.ts";
import { type ReportTimers, WidgetReporter } from "./reports.ts";
import { type KeyValueStorage, WindowStore } from "./window-store.ts";

/** What the SDK needs from `@omfp/client`'s `Client` (tests pass a fake over a real `Model`). */
export interface SdkClient extends ModelSource {
  call<M extends CallMethod>(method: M, params: ParamsOf<M>): Promise<ResultOf<M>>;
  subscribe(topic: Topic): void;
  unsubscribe(topic: Topic): void;
}

/** Topics followed while at least one widget needs them. */
export class TopicRefs {
  readonly #client: SdkClient;
  readonly #counts = new Map<Topic, number>();

  constructor(client: SdkClient) {
    this.#client = client;
  }

  /** Follows `topic` until the returned function is called (as many times as it was retained). */
  retain(topic: Topic): () => void {
    const count = this.#counts.get(topic) ?? 0;
    this.#counts.set(topic, count + 1);
    if (count === 0) {
      this.#client.subscribe(topic);
    }
    let released = false;
    return () => {
      if (released) {
        return;
      }
      released = true;
      const left = (this.#counts.get(topic) ?? 1) - 1;
      if (left === 0) {
        this.#counts.delete(topic);
        this.#client.unsubscribe(topic);
      } else {
        this.#counts.set(topic, left);
      }
    };
  }
}

/** The latest message with a counter, so repeating a message is announced again. */
export interface Announcement {
  readonly message: string;
  readonly count: number;
}

/** Polite announcements for screen readers (UI-092), rendered by the frame's live region. */
export class Announcer {
  #latest: Announcement = { message: "", count: 0 };
  readonly #listeners = new Set<() => void>();

  readonly getSnapshot = (): Announcement => this.#latest;

  announce(message: string): void {
    this.#latest = { message, count: this.#latest.count + 1 };
    for (const listener of [...this.#listeners]) {
      listener();
    }
  }

  readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  };
}

export interface OmfpHost {
  readonly client: SdkClient;
  readonly frames: FrameStore;
  readonly topics: TopicRefs;
  readonly window: WindowStore;
  readonly commands: CommandRegistry;
  readonly announcer: Announcer;
  /** Workspace and user widget modules, imported once per URL (UI-105). */
  readonly modules: WidgetModules;
  /** This window's widget reports to the daemon (AGT-026). */
  readonly reports: WidgetReporter;
}

export interface HostOptions {
  /** Default: `requestAnimationFrame`. */
  readonly schedule?: FrameScheduler;
  /** Where persisted window state lives; default: none (tests), the frame passes `localStorage`. */
  readonly storage?: KeyValueStorage;
  /** How widget modules are imported; default: the browser's `import()` (tests pass a fake). */
  readonly importer?: Importer;
  /** How widget reports are batched; default: the global timers. */
  readonly reportTimers?: ReportTimers;
}

/** A host over `client`; call `host.frames.start()` to follow the model. */
export function createHost(client: SdkClient, options: HostOptions = {}): OmfpHost {
  return {
    client,
    frames: new FrameStore(client, options.schedule),
    topics: new TopicRefs(client),
    window: new WindowStore(options.storage),
    commands: new CommandRegistry(),
    announcer: new Announcer(),
    modules: new WidgetModules(options.importer ?? browserImporter, strings.widgets.noDefaultExport),
    reports: new WidgetReporter({ report: (params) => client.call("widgets.report", params) }, options.reportTimers),
  };
}

const HostContext = createContext<OmfpHost | undefined>(undefined);

export function OmfpProvider({ host, children }: { host: OmfpHost; children: ReactNode }): ReactNode {
  return <HostContext value={host}>{children}</HostContext>;
}

/** The window's host; throws outside {@link OmfpProvider}. */
export function useHost(): OmfpHost {
  const host = useContext(HostContext);
  if (host === undefined) {
    throw new Error("useHost outside <OmfpProvider>");
  }
  return host;
}
