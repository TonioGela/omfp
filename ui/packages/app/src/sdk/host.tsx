/**
 * The host every widget runs in: the protocol client, the per-frame model snapshots, window-local state, the command
 * registry, the live region and the widget modules. The frame creates one per window and provides it with {@link OmfpProvider}; widgets
 * reach it only through the hooks of `./hooks.ts`.
 */

import type { CallMethod } from "@omfp/client";
import {
  INDEX_TOPIC,
  type ParamsOf,
  parseTopic,
  type ResultOf,
  type Topic,
  type TransientFrame,
  type WindowRequestFrame,
} from "@omfp/protocol";
import { createContext, type ReactNode, useContext } from "react";
import { strings } from "../strings/index.ts";
import { CommandRegistry } from "./commands.ts";
import { type FilePoll, intervalPoll } from "./files.ts";
import { type FrameScheduler, FrameStore, type ModelSource, type Snapshot } from "./frame-store.ts";
import { InitFiles } from "./init.ts";
import { browserLiveMedia, LiveClient, type LiveMedia } from "./live.ts";
import { browserImporter, type Importer, WidgetModules } from "./modules.ts";
import { Notices, type NoticeTimers } from "./notify.ts";
import { type ReportTimers, WidgetReporter } from "./reports.ts";
import { ResultCache } from "./results.ts";
import { type KeyValueStorage, WindowStore } from "./window-store.ts";

/** What the SDK needs from `@omfp/client`'s `Client` (tests pass a fake over a real `Model`). */
export interface SdkClient extends ModelSource {
  call<M extends CallMethod>(method: M, params: ParamsOf<M>): Promise<ResultOf<M>>;
  /** Follows `topic`; a no-op while it is followed, unless its subscription failed (then it is retried). */
  subscribe(topic: Topic): void;
  unsubscribe(topic: Topic): void;
  /** Transient frames of followed topics (PRT-054) until the returned function is called. */
  onTransient(listener: (frame: TransientFrame) => void): () => void;
  /** The daemon's requests to this window (PRT-124) until the returned function is called. */
  onWindowRequest(listener: (frame: WindowRequestFrame) => void): () => void;
}

/** How many threads stay followed after no widget shows them any more (UI-335). */
export const RECENT_THREADS = 5;

/** Topics followed while at least one widget needs them. A `thread/<id>` topic stays followed after its last release,
 * among the {@link RECENT_THREADS} most recently released ones, so showing that thread again needs no new snapshot: its
 * state is already in the model and kept live (UI-335). A kept thread gone from the index (deleted) is released. */
export class TopicRefs {
  readonly #client: SdkClient;
  readonly #counts = new Map<Topic, number>();
  /** Released thread topics still followed, oldest first. */
  readonly #recent = new Set<Topic>();

  constructor(client: SdkClient, frames: FrameStore) {
    this.#client = client;
    frames.subscribe(() => this.#prune(frames.getSnapshot()));
  }

  /** Follows `topic` until the returned function is called (as many times as it was retained). */
  retain(topic: Topic): () => void {
    const count = this.#counts.get(topic) ?? 0;
    this.#counts.set(topic, count + 1);
    if (count === 0) {
      this.#recent.delete(topic);
      // A no-op for a kept topic, unless its subscription failed: then it is retried.
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
        this.#keep(topic);
      } else {
        this.#counts.set(topic, left);
      }
    };
  }

  /** Keeps a released thread topic followed (the oldest kept one goes over the bound); unsubscribes anything else. */
  #keep(topic: Topic): void {
    if (parseTopic(topic)?.kind !== "thread") {
      this.#client.unsubscribe(topic);
      return;
    }
    this.#recent.add(topic);
    for (const oldest of this.#recent) {
      if (this.#recent.size <= RECENT_THREADS) {
        break;
      }
      this.#recent.delete(oldest);
      this.#client.unsubscribe(oldest);
    }
  }

  /** Releases the kept threads the live index no longer lists. */
  #prune(snapshot: Snapshot): void {
    if (this.#recent.size === 0 || !snapshot.dirty.index) {
      return;
    }
    const index = snapshot.topics.find((state) => state.topic === INDEX_TOPIC);
    if (index?.sync.kind !== "live") {
      return;
    }
    for (const topic of this.#recent) {
      const parsed = parseTopic(topic);
      if (parsed?.kind === "thread" && !snapshot.index.threads.has(parsed.id)) {
        this.#recent.delete(topic);
        this.#client.unsubscribe(topic);
      }
    }
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
  /** How open files are checked for changes (`useFile`, UI-110). */
  readonly filePoll: FilePoll;
  /** This window's live voice session (UI-180..UI-186). */
  readonly live: LiveClient;
  /** Notices to the user: the mode line or a system notification (UI-192, UI-194). */
  readonly notices: Notices;
  /** The user's and the current workspace's init files (UI-260..UI-263). */
  readonly init: InitFiles;
  /** The data hooks' last results, shown again while a widget mounted again refreshes them (UI-392). */
  readonly results: ResultCache;
}

export interface HostOptions {
  /** Default: `requestAnimationFrame`. */
  readonly schedule?: FrameScheduler;
  /** Where persisted window state lives; default: none (tests), the frame passes `localStorage`. */
  readonly storage?: KeyValueStorage;
  /** How widget modules and init files are imported; default: the browser's `import()` (tests pass a fake). */
  readonly importer?: Importer;
  /** How widget reports are batched; default: the global timers. */
  readonly reportTimers?: ReportTimers;
  /** When mode-line notices expire; default: the global timers. */
  readonly noticeTimers?: NoticeTimers;
  /** How open files are checked for changes; default: every 2 s (tests pass a manual one). */
  readonly filePoll?: FilePoll;
  /** The browser APIs of live voice; default: the browser's (tests pass a fake peer and microphone). */
  readonly liveMedia?: LiveMedia;
  /** The window runs on the daemon's machine (`#local=1`, UI-188): without WebRTC it starts omp's own live session. */
  readonly local?: boolean;
}

/** A host over `client`; call `host.frames.start()` to follow the model. */
export function createHost(client: SdkClient, options: HostOptions = {}): OmfpHost {
  const frames = new FrameStore(client, options.schedule);
  const topics = new TopicRefs(client, frames);
  const window = new WindowStore(options.storage);
  const importer = options.importer ?? browserImporter;
  const host: OmfpHost = {
    client,
    frames,
    topics,
    window,
    commands: new CommandRegistry(),
    announcer: new Announcer(),
    modules: new WidgetModules(importer, strings.widgets.noDefaultExport),
    reports: new WidgetReporter({ report: (params) => client.call("widgets.report", params) }, options.reportTimers),
    filePoll: options.filePoll ?? intervalPoll,
    live: new LiveClient({
      client,
      frames,
      topics,
      window,
      media: options.liveMedia ?? browserLiveMedia(options.local ?? false),
    }),
    notices: new Notices(window, options.noticeTimers),
    init: new InitFiles(() => host, importer),
    results: new ResultCache(),
  };
  return host;
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
