/** The gallery's chat (UI-173): the transcript, composers and the live voice panel. */
import {
  Button,
  ChatHeader,
  Chip,
  Composer,
  ComposerBar,
  ComposerHint,
  Icon,
  IconButton,
  Kbd,
  type Levels,
  LivePanel,
  Markdown,
  Message,
  QueueChip,
  Select,
  SILENT_SOURCE,
  Spectrum,
  type SpectrumSource,
  StreamCaret,
  Thinking,
  VoiceMeter,
} from "@omfp/kit";
import { useState } from "react";
import { Line, MODELS, Section } from "./parts.tsx";

export function Transcript() {
  const [open, setOpen] = useState(true);
  return (
    <Section title="Transcript" note="UI-171 · UI-147 · UI-338" wide>
      <ChatHeader title="Redesign kit tokens">
        <Select
          label="Model"
          hideLabel
          icon={<Icon name="brain" />}
          items={MODELS}
          selectedKey="opus-4.5"
          onSelectionChange={() => undefined}
        />
        <Chip icon={<Icon name="link-2" />}>pmo</Chip>
      </ChatHeader>
      <div className="omfp-gallery__transcript">
        <Message author="user" name="you">
          <p className="omfp-prose">Move the kit's tokens into one namespace and restyle the buttons.</p>
        </Message>
        <Thinking label="Thinking" meta="4s" expanded={false} />
        <Thinking label="Thinking" meta="12s" expanded={open} onExpandedChange={setOpen}>
          The tokens live in two places; the kit's ad-hoc ones go away without aliases. The buttons read their colors
          from the kit's own names, so the rename touches every rule that draws one: the primary gradient, the danger
          tone and the focus ring. A reader of the transcript skims this reasoning, so it stays light and secondary
          beside the reply below, which carries the answer.
        </Thinking>
        {/* A transcript item with the keyboard focus (UI-132): the tint and the bar beside the message. */}
        <div className="omfp-focus-item omfp-gallery__item is-focus">
          <Message author="assistant" name="omp">
            <Markdown text={"Done: the tokens now live in **one namespace**, `--omfp-*`."} />
          </Message>
        </div>
        <Message author="assistant" name="omp">
          <p className="omfp-prose">
            Restyling the buttons now, starting with the primary gradient
            <StreamCaret />
          </p>
        </Message>
        <Message author="assistant" name="omp" error="The model stopped: rate limited (429).">
          <p className="omfp-prose">Checking the contrast of</p>
        </Message>
      </div>
    </Section>
  );
}

export function Composers() {
  return (
    <Section title="Composer" note="UI-172" wide>
      <div className="omfp-gallery__pair">
        <Composer>
          <textarea
            className="omfp-prose omfp-compose__input"
            rows={2}
            aria-label="Normal"
            placeholder="Message omp…"
          />
          <ComposerBar>
            <ComposerHint>
              <Kbd>i</Kbd> write
            </ComposerHint>
            <Button variant="primary" size="sm" icon={<Icon name="arrow-up" />} kbd="enter">
              Send
            </Button>
          </ComposerBar>
        </Composer>
        <Composer mode="insert">
          <textarea
            className="omfp-prose omfp-compose__input"
            rows={2}
            aria-label="Insert"
            defaultValue="Insert mode: the composer border changes"
          />
          <ComposerBar>
            <Chip icon={<Icon name="paperclip" />} onRemove={() => undefined} removeLabel="Remove attachment">
              shot.png
            </Chip>
            <ComposerHint>
              <Kbd>ctrl+enter</Kbd> queue
            </ComposerHint>
            <Button variant="primary" size="sm" icon={<Icon name="arrow-up" />} kbd="enter">
              Send
            </Button>
          </ComposerBar>
        </Composer>
      </div>
      <div className="omfp-gallery__pair">
        <div className="omfp-gallery__stack">
          <div className="omfp-gallery__line">
            <QueueChip kind="follow-up" onRemove={() => undefined}>
              and a follow-up after that
            </QueueChip>
            <QueueChip kind="steer">use the gradient tokens</QueueChip>
          </div>
          <Composer mode="insert">
            <textarea
              className="omfp-prose omfp-compose__input"
              rows={2}
              aria-label="Busy"
              defaultValue="One more thing"
            />
            <ComposerBar>
              <ComposerHint>
                <Kbd>enter</Kbd> queue
              </ComposerHint>
              <Button variant="danger" size="sm" icon={<Icon name="square" />} kbd="ctrl+c">
                Abort
              </Button>
            </ComposerBar>
          </Composer>
        </div>
        <Composer disabled>
          <textarea
            className="omfp-prose omfp-compose__input"
            rows={2}
            aria-label="Disabled"
            disabled
            placeholder="The thread is archived"
          />
          <ComposerBar>
            <Button variant="primary" size="sm" icon={<Icon name="arrow-up" />} isDisabled>
              Send
            </Button>
          </ComposerBar>
        </Composer>
      </div>
    </Section>
  );
}

/** A still voice for the gallery: fixed, speech-shaped bars (a low-mid hump and sibilants) and a fixed level. */
class StillVoice implements SpectrumSource {
  readonly #voice: "user" | "assistant";

  constructor(voice: "user" | "assistant") {
    this.#voice = voice;
  }

  read(mic: Float32Array, remote: Float32Array): Levels {
    const out = this.#voice === "user" ? mic : remote;
    (this.#voice === "user" ? remote : mic).fill(0);
    const center = this.#voice === "user" ? 0.22 : 0.34;
    for (let bar = 0; bar < out.length; bar++) {
      const x = bar / (out.length - 1);
      const hump = 0.78 * Math.exp(-((x - center) ** 2) / 0.012) + 0.32 * Math.exp(-((x - 0.62) ** 2) / 0.006);
      out[bar] = Math.min(1, 0.04 + hump * (0.82 + 0.18 * Math.sin(bar * 1.7)));
    }
    return this.levels();
  }

  levels(): Levels {
    return this.#voice === "user" ? { mic: 0.09, remote: 0 } : { mic: 0, remote: 0.16 };
  }
}

const USER_VOICE = new StillVoice("user");
const ASSISTANT_VOICE = new StillVoice("assistant");
const LIVE_KEYS = [
  { keys: "spc", label: "mute" },
  { keys: "esc", label: "end" },
];
const LIVE_LINES = [
  { role: "user", text: "Ciao, mi senti? Dimmi solo sì." },
  { role: "assistant", text: "Sì, ti sento: dimmi pure." },
] as const;

/** The live panel in each state; the still voices draw the canvas at motion `full` (deterministic: fixed bars). */
export function LiveVoice() {
  return (
    <Section title="Live voice" note="UI-182 · UI-184 · spectrum at full, meter below" wide>
      <div className="omfp-gallery__pair">
        <LivePanel
          title="Live voice"
          status="listening"
          statusLabel="listening"
          spectrum={<Spectrum motion="full" source={USER_VOICE} status="listening" label="Voice level" />}
          lines={LIVE_LINES.slice(0, 1)}
          hints={LIVE_KEYS}
        />
        <LivePanel
          title="Live voice"
          status="speaking"
          statusLabel="speaking"
          spectrum={<Spectrum motion="full" source={ASSISTANT_VOICE} status="speaking" label="Voice level" />}
          lines={LIVE_LINES}
          hints={LIVE_KEYS}
          className="is-focus"
        />
      </div>
      <div className="omfp-gallery__pair">
        <LivePanel
          title="Live voice"
          status="muted"
          statusLabel="muted"
          spectrum={<Spectrum motion="full" source={USER_VOICE} status="muted" label="Voice level" />}
          lines={LIVE_LINES}
          hints={[{ keys: "spc", label: "unmute" }, LIVE_KEYS[1] ?? { keys: "esc", label: "end" }]}
        />
        <LivePanel
          title="Live voice"
          status="connecting"
          statusLabel="connecting"
          spectrum={<Spectrum source={SILENT_SOURCE} status="connecting" label="Voice level" />}
          hints={LIVE_KEYS}
        />
      </div>
      <div className="omfp-gallery__pair">
        <LivePanel
          title="Live voice"
          status="working"
          statusLabel="working"
          lines={LIVE_LINES.slice(1)}
          note="in another window (omfp/web)"
          actions={
            <Button variant="danger" size="sm" icon={<Icon name="square" />}>
              End
            </Button>
          }
        />
        <LivePanel
          title="Live voice"
          status="error"
          statusLabel="error"
          message="live voice needs an `openai-codex` login in omp: log in with `/login openai-codex` in omp, then try again"
          hints={[{ keys: "esc", label: "dismiss" }]}
          actions={<IconButton label="Dismiss" icon={<Icon name="x" />} />}
        />
      </div>
      <Line label="meter">
        <VoiceMeter source={USER_VOICE} status="listening" label="Microphone" className="omfp-gallery__meter" />
        <VoiceMeter source={ASSISTANT_VOICE} status="speaking" label="Assistant" className="omfp-gallery__meter" />
        <VoiceMeter source={USER_VOICE} status="muted" label="Muted" className="omfp-gallery__meter" />
      </Line>
      <Line label="mic">
        <IconButton label="Start live voice" icon={<Icon name="mic" />} />
        <IconButton label="End live voice" icon={<Icon name="mic" />} aria-pressed />
      </Line>
    </Section>
  );
}
