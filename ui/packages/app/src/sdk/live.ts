/**
 * Live voice in the browser (UI-180..UI-186, spike S-09): {@link LiveClient} holds this window's one session — the
 * browser half of omp's `live_connect`: the microphone, a WebRTC peer whose offer the daemon relays
 * (`thread.live_connect`), the remote voice on an `<audio>` element and the analysers the spectrum draws. omp cannot
 * see the peer, so the window sends `thread.live_stop` when the peer fails or closes, on the end key, on a thread switch
 * and on unload (UI-185). The browser parts sit behind {@link LiveMedia}, which tests replace.
 */

import { CallError } from "@omfp/client";
import { AnalyserSource, type Levels, SILENT_SOURCE, type SpectrumSource, Tap } from "@omfp/kit";
import { threadTopic } from "@omfp/protocol";
import { strings } from "../strings/index.ts";
import { IDLE, type LiveSupport, type LocalLive } from "../vm/live.ts";
import type { FrameStore } from "./frame-store.ts";
import type { SdkClient, TopicRefs } from "./host.tsx";
import { SELECTED_THREAD } from "./keys.ts";
import type { WindowStore } from "./window-store.ts";

/** The remote voice counts as speaking above this RMS … */
const SPEAKING_LEVEL = 0.015;
/** … and for this long after (UI-184). */
const SPEAKING_HOLD_MS = 350;

/** The browser half of one session, open: the microphone and a peer that sends it, with its offer. */
export interface LiveLink {
  /** The peer's SDP offer (one sendrecv Opus audio track and the `oai-events` data channel). */
  readonly offer: string;
  /** The analysers of the microphone and (once it arrives) the remote voice. */
  readonly source: SpectrumSource;
  /** Applies omp's SDP answer. */
  answer(sdp: string): Promise<void>;
  /** Enables or disables the local track (UI-183). */
  setMuted(muted: boolean): void;
  /** Closes the peer, the microphone, the remote audio and the analysers. */
  close(): void;
}

/** Why opening a session failed: the microphone (permission, no device) or the peer. */
export class LiveOpenError extends Error {
  readonly stage: "microphone" | "peer";

  constructor(stage: "microphone" | "peer", message: string) {
    super(message);
    this.name = "LiveOpenError";
    this.stage = stage;
  }
}

/** The browser APIs of a session; {@link browserLiveMedia} in the app, a fake in tests. */
export interface LiveMedia {
  support(): LiveSupport;
  /** Opens the microphone and a peer sending it; `onState` gets the peer's `connectionState` changes. */
  open(onState: (state: RTCPeerConnectionState) => void): Promise<LiveLink>;
  /** The clock of the speaking hold (ms). */
  now(): number;
}

/** The session and the errors the user dismissed, by thread. */
export interface LiveSnapshot {
  readonly local: LocalLive;
  readonly dismissed: Readonly<Record<string, string>>;
}

export interface LiveClientOptions {
  readonly client: SdkClient;
  readonly frames: FrameStore;
  readonly topics: TopicRefs;
  readonly window: WindowStore;
  readonly media: LiveMedia;
}

/** The readable message of a failure: the daemon's message (DMN-104), else the error's. */
function failureMessage(error: unknown): string {
  if (error instanceof CallError && error.failure.kind === "daemon") {
    return error.failure.error.message;
  }
  return error instanceof Error ? error.message : String(error);
}

/** This window's live session (at most one), read by the widgets through {@link LiveClient.subscribe}. */
export class LiveClient {
  readonly #client: SdkClient;
  readonly #frames: FrameStore;
  readonly #topics: TopicRefs;
  readonly #window: WindowStore;
  readonly #media: LiveMedia;
  #snapshot: LiveSnapshot = { local: IDLE, dismissed: {} };
  readonly #listeners = new Set<() => void>();
  /** The open session's link, and what it holds while open. */
  #link: LiveLink | undefined;
  #release: (() => void)[] = [];
  /** The thread state showed this window's session: from then on its absence means it ended. */
  #seen = false;
  /** The open session's peer reached `connected`. */
  #connected = false;
  #speakingUntil = 0;

  constructor(options: LiveClientOptions) {
    this.#client = options.client;
    this.#frames = options.frames;
    this.#topics = options.topics;
    this.#window = options.window;
    this.#media = options.media;
  }

  readonly getSnapshot = (): LiveSnapshot => this.#snapshot;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.#listeners.add(listener);
    return () => {
      this.#listeners.delete(listener);
    };
  };

  support(): LiveSupport {
    return this.#media.support();
  }

  /** What the spectrum draws: the open session's analysers, else silence. */
  source(): SpectrumSource {
    return this.#link?.source ?? SILENT_SOURCE;
  }

  /**
   * Starts a session on `threadId` (UI-181): microphone, peer and offer, `thread.live_connect`, then omp's answer.
   * Ends this window's other session first. A failure shows in the panel (UI-186).
   */
  async start(threadId: string): Promise<void> {
    const support = this.#media.support();
    if (support !== "available") {
      this.#set({ kind: "failed", threadId, message: strings.chat.live[support] });
      return;
    }
    this.end();
    const starting: LocalLive = { kind: "starting", threadId };
    this.#set(starting);
    this.#seen = false;
    this.#connected = false;
    this.#speakingUntil = 0;
    let link: LiveLink | undefined;
    try {
      link = await this.#media.open((state) => this.#peerState(link, state));
    } catch (error) {
      if (this.#snapshot.local === starting) {
        const stage = error instanceof LiveOpenError ? error.stage : "peer";
        this.#set({ kind: "failed", threadId, message: strings.chat.live[stage](failureMessage(error)) });
      }
      return;
    }
    if (this.#snapshot.local !== starting) {
      link.close();
      return;
    }
    this.#open(threadId, link);
    let answer: string;
    try {
      ({ sdpAnswer: answer } = await this.#client.call("thread.live_connect", { threadId, sdpOffer: link.offer }));
    } catch (error) {
      if (this.#link === link) {
        this.#close();
        this.#set({ kind: "failed", threadId, message: failureMessage(error) });
      }
      return;
    }
    if (this.#link !== link) {
      return;
    }
    try {
      await link.answer(answer);
    } catch (error) {
      if (this.#link === link) {
        this.#stop(threadId);
        this.#close();
        this.#set({ kind: "failed", threadId, message: strings.chat.live.peer(failureMessage(error)) });
      }
      return;
    }
    if (this.#link === link) {
      this.#set({ kind: "running", threadId, connected: this.#connected, speaking: false, muted: false });
      this.#check();
    }
  }

  /** Ends this window's session (the end key, a thread switch, unload, UI-185): `thread.live_stop`, then closes it. */
  end(): void {
    const local = this.#snapshot.local;
    if (local.kind !== "starting" && local.kind !== "running") {
      return;
    }
    // A session still starting may reach omp yet: stop it either way.
    this.#close();
    this.#set(IDLE);
    this.#stop(local.threadId);
  }

  /** Ends the session of thread `threadId`, this window's or another's (UI-182). */
  endThread(threadId: string): void {
    const local = this.#snapshot.local;
    if ((local.kind === "starting" || local.kind === "running") && local.threadId === threadId) {
      this.end();
    } else {
      this.#stop(threadId);
    }
  }

  /** Mutes or unmutes this window's running session (UI-183): the local track, then `thread.live_mute`. */
  toggleMute(): void {
    const local = this.#snapshot.local;
    if (local.kind !== "running") {
      return;
    }
    const muted = !local.muted;
    this.#link?.setMuted(muted);
    this.#set({ ...local, muted });
    this.#client.call("thread.live_mute", { threadId: local.threadId, muted }).catch(() => {
      // omp refuses without a session (`conflict`): the session ended meanwhile and its `live_end` closes it.
    });
  }

  /**
   * The levels the spectrum read (every frame, or four times a second below motion `full`): `speaking` while the
   * remote voice is audible (UI-184; omp reports no `speaking` phase for `live_connect`).
   */
  readonly noteLevels = (levels: Levels): void => {
    const local = this.#snapshot.local;
    if (local.kind !== "running") {
      return;
    }
    const now = this.#media.now();
    if (levels.remote > SPEAKING_LEVEL) {
      this.#speakingUntil = now + SPEAKING_HOLD_MS;
    }
    const speaking = now < this.#speakingUntil;
    if (speaking !== local.speaking) {
      this.#set({ ...local, speaking });
    }
  };

  /** Hides the failure shown on `threadId`: this window's failed start, and `message`, the thread's `live_error`. */
  dismiss(threadId: string, message: string): void {
    const local = this.#snapshot.local;
    this.#snapshot = {
      local: local.kind === "failed" && local.threadId === threadId ? IDLE : local,
      dismissed: { ...this.#snapshot.dismissed, [threadId]: message },
    };
    this.#emit();
  }

  #open(threadId: string, link: LiveLink): void {
    this.#link = link;
    const unload = () => this.end();
    window.addEventListener("pagehide", unload);
    this.#release = [
      this.#topics.retain(threadTopic(threadId)),
      this.#frames.subscribe(() => this.#check()),
      this.#window.subscribe(SELECTED_THREAD, () => {
        if (this.#window.get(SELECTED_THREAD) !== threadId) {
          this.end();
        }
      }),
      () => window.removeEventListener("pagehide", unload),
    ];
  }

  #close(): void {
    const link = this.#link;
    this.#link = undefined;
    for (const release of this.#release) {
      release();
    }
    this.#release = [];
    link?.close();
  }

  #stop(threadId: string): void {
    this.#client.call("thread.live_stop", { threadId }).catch((error: unknown) => {
      // Shown unless a new session started meanwhile.
      if (this.#snapshot.local.kind === "idle") {
        this.#set({ kind: "failed", threadId, message: strings.chat.live.stopFailed(failureMessage(error)) });
      }
    });
  }

  /** Closes the session once the thread state shows it ended (`live_end`, another owner, UI-185). */
  #check(): void {
    const local = this.#snapshot.local;
    if (local.kind !== "running") {
      return;
    }
    const snapshot = this.#frames.getSnapshot();
    const view = snapshot.threads.get(local.threadId);
    if (view === undefined) {
      return;
    }
    const connection = snapshot.connection.kind === "connected" ? snapshot.connection.hello.connection : undefined;
    const live = view.state.live;
    if (live !== null && live.owner.connection === connection) {
      this.#seen = true;
    } else if (this.#seen) {
      this.#close();
      this.#set(IDLE);
    }
  }

  #peerState(link: LiveLink | undefined, state: RTCPeerConnectionState): void {
    if (link === undefined || this.#link !== link) {
      return;
    }
    if (state === "failed" || state === "closed") {
      // omp cannot see the peer: tell it the call is gone (UI-185).
      this.end();
    } else if (state === "connected") {
      this.#connected = true;
      const local = this.#snapshot.local;
      if (local.kind === "running" && !local.connected) {
        this.#set({ ...local, connected: true });
      }
    }
  }

  #set(local: LocalLive): void {
    this.#snapshot = { ...this.#snapshot, local };
    this.#emit();
  }

  #emit(): void {
    for (const listener of [...this.#listeners]) {
      listener();
    }
  }
}

/** The browser's {@link LiveMedia}: `getUserMedia`, `RTCPeerConnection`, an `<audio>` element and Web Audio. */
export const browserLiveMedia: LiveMedia = {
  support() {
    if (!window.isSecureContext) {
      return "insecure";
    }
    return typeof RTCPeerConnection === "function" && navigator.mediaDevices?.getUserMedia !== undefined
      ? "available"
      : "unsupported";
  },

  now: () => performance.now(),

  async open(onState) {
    let microphone: MediaStream;
    try {
      microphone = await navigator.mediaDevices.getUserMedia({
        audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
      });
    } catch (error) {
      throw new LiveOpenError("microphone", failureMessage(error));
    }
    const source = new AnalyserSource();
    const context = new AudioContext();
    const peer = new RTCPeerConnection();
    const audio = new Audio();
    const close = () => {
      peer.close();
      for (const track of microphone.getTracks()) {
        track.stop();
      }
      audio.pause();
      audio.srcObject = null;
      source.mic?.disconnect();
      source.remote?.disconnect();
      void context.close();
    };
    try {
      // Not awaited: without a user activation `resume` waits for one; the analysers stay silent until then.
      void context.resume().catch(() => undefined);
      source.mic = new Tap(context, context.createMediaStreamSource(microphone));
      const [track] = microphone.getAudioTracks();
      if (track === undefined) {
        throw new Error("the microphone has no audio track");
      }
      peer.addTransceiver(track, { direction: "sendrecv", streams: [microphone] });
      // omp's realtime model talks over the `oai-events` channel; the browser only has to open it.
      peer.createDataChannel("oai-events");
      peer.addEventListener("track", (event) => {
        const stream = event.streams[0] ?? new MediaStream([event.track]);
        // Chromium feeds a remote WebRTC stream into Web Audio only while a media element plays it.
        audio.autoplay = true;
        audio.srcObject = stream;
        void audio.play().catch(() => undefined);
        source.remote = new Tap(context, context.createMediaStreamSource(stream));
      });
      peer.addEventListener("connectionstatechange", () => onState(peer.connectionState));
      await peer.setLocalDescription(await peer.createOffer());
      const offer = peer.localDescription?.sdp;
      if (offer === undefined || offer === "") {
        throw new Error("the browser made no SDP offer");
      }
      return {
        offer,
        source,
        answer: (sdp) => peer.setRemoteDescription({ type: "answer", sdp }),
        setMuted(muted) {
          for (const local of microphone.getAudioTracks()) {
            local.enabled = !muted;
          }
        },
        close,
      };
    } catch (error) {
      close();
      throw new LiveOpenError("peer", failureMessage(error));
    }
  },
};
