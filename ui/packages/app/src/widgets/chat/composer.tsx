/**
 * `omfp:composer`: queue chips (UI-062) and the composer (UI-060..064) of the selected thread. Multi-line input with
 * image paste/drop, large pastes as chips (UI-357..UI-360), `/` command completion (React Aria `Autocomplete` +
 * `TextArea` + `Menu` in a non-modal `Popover`, S-11), the send mode on the send button, Esc aborting a busy thread or
 * leaving the composer, `Ctrl-C` clearing the draft (restored by `u`) then aborting without restoring the queue
 * (UI-063), and drafts per thread. With no thread selected, the composer of the draft thread (UI-023): its first
 * message creates the thread.
 */

import type { AvailableSlashCommand, ImageContent } from "@omfp/core";
import {
  Autocomplete,
  Button,
  ComposerBar,
  ComposerHint,
  Icon,
  Kbd,
  Composer as KitComposer,
  Menu,
  MenuItem,
  Popover,
  TextArea,
} from "omfp/kit";
import {
  alternative,
  behavior,
  canAbort,
  commandsOf,
  complete,
  DRAFT_MESSAGE,
  DRAFT_THREAD,
  type DraftThread,
  draftKey,
  EMPTY_DRAFT,
  expandPastes,
  FOCUS_COMPOSER,
  INTERRUPT_WINDOW_MS,
  imageAttachment,
  interruptAction,
  isBlank,
  isBusy,
  keyAction,
  keyLabel,
  type Modifiers,
  rawPasteKeys,
  restoreCleared,
  restoreDraft,
  restoredTexts,
  restoreFailedPrompt,
  SCOPE_ATTRIBUTE,
  type SendMode,
  type Suggestion,
  sendMode,
  slashSuggestions,
  steerModifier,
  strings,
  useAnnounce,
  useCall,
  useCommand,
  useHost,
  useThread,
  useThreadActions,
  useWindowState,
} from "omfp/sdk";
import {
  type ClipboardEvent,
  type DragEvent,
  type KeyboardEvent,
  memo,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { Attachments } from "./attachments.tsx";
import { usePromptHistory } from "./history.ts";
import { LiveVoicePanel, MicButton } from "./live.tsx";
import { PasteChips, usePastes } from "./pastes.tsx";
import { testId, type WidgetProps } from "./props.ts";
import { QueueChips } from "./queue.tsx";
import { clearedDraftKey, useClock, useSelectedThread } from "./state.ts";

const text = strings.chat.composer;

export function ComposerWidget(_props: WidgetProps) {
  const { threadId, view } = useSelectedThread();
  const [draft] = useWindowState(DRAFT_THREAD);
  if (threadId === undefined && draft !== undefined) {
    return (
      <div className="omfp-composer-area">
        <DraftComposer draft={draft} />
      </div>
    );
  }
  if (threadId === undefined || view === undefined) {
    return null;
  }
  const { state } = view;
  return (
    <div className="omfp-composer-area">
      <LiveVoicePanel threadId={threadId} state={state} />
      <QueueChips threadId={threadId} state={state} />
      <Composer
        key={threadId}
        threadId={threadId}
        busy={isBusy(state)}
        abortable={canAbort(state)}
        commands={state.commands}
      />
    </div>
  );
}

/**
 * The draft thread's composer (UI-023): Enter (or the button) creates the thread with the text as first message; the
 * text stays when that fails. It takes the focus whenever the draft is opened; Esc leaves it, discarding an empty
 * draft.
 */
function DraftComposer({ draft }: { draft: DraftThread }) {
  const actions = useThreadActions();
  const [message, setMessage] = useWindowState(DRAFT_MESSAGE);
  const [sending, setSending] = useState(false);
  /** The input has the focus: the keymap is in insert mode (UI-098), drawn as the gradient border (UI-172). */
  const [insert, setInsert] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const labels = strings.shell.draft;
  // biome-ignore lint/correctness/useExhaustiveDependencies: `draft` is a new object each time the draft is opened
  useEffect(() => {
    inputRef.current?.focus();
  }, [draft]);
  const send = () => {
    if (sending || isBlank(message)) {
      return;
    }
    setSending(true);
    void actions.sendDraft().then((sent) => {
      if (!sent) {
        setSending(false);
      }
    });
  };
  const leave = () => {
    if (isBlank(message)) {
      actions.discardDraft();
    }
    inputRef.current?.blur();
  };
  useCommand({ id: "composer.focus", title: text.focus, run: () => inputRef.current?.focus() });
  useCommand({ id: "focus.leave", title: text.leave, scope: "composer", run: leave });
  const pastes = usePastes(message, setMessage, inputRef);
  const onKeyDownCapture = (event: KeyboardEvent<HTMLElement>) => {
    if (event.target !== inputRef.current || event.nativeEvent.isComposing) {
      return;
    }
    pastes.onKeyDown(event, modifiersOf(event));
    const action = keyAction(event.key, modifiersOf(event), {
      busy: false,
      abortable: false,
      hasSuggestions: false,
      repeat: event.repeat,
    });
    if (action.type === "send" || action.type === "leave" || action.type === "ignore") {
      event.preventDefault();
      event.stopPropagation();
      if (action.type === "send") {
        send();
      } else if (action.type === "leave") {
        leave();
      }
    } else if (action.type === "newline") {
      event.preventDefault();
      event.stopPropagation();
      insertLineBreak(inputRef.current, pastes.onChange);
    }
  };
  return (
    <form
      className="omfp-composer"
      data-testid="composer"
      data-draft="true"
      data-workspace-id={draft.workspaceId}
      {...{ [SCOPE_ATTRIBUTE]: "composer" }}
      onSubmit={(event) => {
        event.preventDefault();
        send();
      }}
      onKeyDownCapture={onKeyDownCapture}
    >
      <KitComposer mode={insert ? "insert" : "normal"}>
        {message.pastes.length > 0 && <PasteChips draft={message} setDraft={setMessage} inputRef={inputRef} />}
        <TextArea
          className="omfp-compose__field"
          label={text.label}
          hideLabel
          rows={2}
          placeholder={labels.placeholder}
          value={message.text}
          onChange={pastes.onChange}
          onFocus={() => setInsert(true)}
          onBlur={() => setInsert(false)}
          isReadOnly={sending}
          inputRef={inputRef}
          inputProps={{
            ...testId("composer-input"),
            "aria-keyshortcuts": `Enter Alt+Enter Escape ${ariaKeys(rawPasteKeys())}`,
            onPaste: pastes.onPaste,
          }}
        />
        <ComposerBar>
          <ComposerHint>
            <Kbd>{keyLabel("Alt-Enter")}</Kbd> {text.hint.newline}
          </ComposerHint>
          <ComposerHint>
            <Kbd>{keyLabel("Esc")}</Kbd> {text.hint.leave}
          </ComposerHint>
          <Button
            type="submit"
            variant="primary"
            size="sm"
            icon={<Icon name="arrow-up" />}
            kbd={keyLabel("Enter")}
            data-testid="send"
            data-mode="draft"
            isDisabled={sending}
          >
            {labels.send}
          </Button>
        </ComposerBar>
      </KitComposer>
    </form>
  );
}

function sendKey(mode: SendMode): string {
  return keyLabel(mode === "steer" ? "Shift-Enter" : "Enter");
}

/**
 * Alt+Enter (UI-060): a line break replaces the selection, the caret after it. The field's value and caret are set
 * first so React, finding the same value, leaves the caret where it is.
 */
function insertLineBreak(input: HTMLTextAreaElement, apply: (text: string) => void) {
  const { selectionStart, selectionEnd, value } = input;
  const text = `${value.slice(0, selectionStart)}\n${value.slice(selectionEnd)}`;
  input.value = text;
  input.setSelectionRange(selectionStart + 1, selectionStart + 1);
  apply(text);
}

function modifiersOf(event: KeyboardEvent): Modifiers {
  return { shift: event.shiftKey, alt: event.altKey, ctrl: event.ctrlKey, meta: event.metaKey };
}

/** `Ctrl-Shift-v` as `aria-keyshortcuts` writes it: `Control+Shift+V`. */
function ariaKeys(keys: string): string {
  return keys
    .split("-")
    .map((part) => (part === "Ctrl" ? "Control" : part.length === 1 ? part.toUpperCase() : part))
    .join("+");
}

/** The image attachments among `files` (UI-060). */
async function attachments(files: readonly File[]): Promise<ImageContent[]> {
  const images = await Promise.all(
    files.map(async (file) => imageAttachment(file.type, new Uint8Array(await file.arrayBuffer()))),
  );
  return images.filter((image) => image !== undefined);
}

/**
 * The thread's `/` commands: those of the last `available_commands_update` (UI-060), or `thread.commands` asked once
 * while the state has none yet.
 */
function useThreadCommands(
  threadId: string,
  known: readonly AvailableSlashCommand[],
): readonly AvailableSlashCommand[] {
  const call = useCall();
  const [fetched, setFetched] = useState<readonly AvailableSlashCommand[]>([]);
  const missing = known.length === 0;
  useEffect(() => {
    if (!missing) {
      return;
    }
    let live = true;
    call("thread.commands", { threadId }).then(
      (result) => {
        if (live) {
          setFetched(commandsOf(result));
        }
      },
      // Without the list, `/` simply offers nothing until the next `available_commands_update`.
      () => {},
    );
    return () => {
      live = false;
    };
  }, [call, missing, threadId]);
  return missing ? fetched : known;
}

/**
 * The composer of one thread; mount it with `key={threadId}`. It takes the few facts of the thread state it needs,
 * so a thread event (a streamed delta) does not render it again, and a key press renders the input but not the
 * action bar (UI-197).
 */
export const Composer = memo(function Composer({
  threadId,
  busy,
  abortable,
  commands: known,
}: {
  threadId: string;
  busy: boolean;
  abortable: boolean;
  commands: readonly AvailableSlashCommand[];
}) {
  const host = useHost();
  const call = useCall();
  const announce = useAnnounce();
  const clock = useClock();
  const [draft, setDraft] = useWindowState(draftKey(threadId));
  const [cleared, setCleared] = useWindowState(clearedDraftKey(threadId));
  /** When `Ctrl-C` last cleared the draft: a second press within {@link INTERRUPT_WINDOW_MS}, with no typing in
   * between, aborts (UI-063). */
  const lastInterrupt = useRef<number | undefined>(undefined);
  /** "Ctrl-C again to abort" shows while a second press would abort. */
  const [interruptHint, setInterruptHint] = useState(false);
  const [steer, setSteer] = useState(false);
  /** Esc closed the suggestions; typing opens them again. */
  const [dismissed, setDismissed] = useState(false);
  /** The input has the focus: the keymap is in insert mode (UI-098), drawn as the gradient border (UI-172). */
  const [insert, setInsert] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const [focusRequest, setFocusRequest] = useWindowState(FOCUS_COMPOSER);
  // Asked to take the focus once shown, e.g. after `g n` (UI-286).
  useEffect(() => {
    if (focusRequest === threadId) {
      inputRef.current?.focus();
      setFocusRequest(undefined);
    }
  }, [focusRequest, threadId, setFocusRequest]);
  const mode = sendMode(busy, steer);
  const other = alternative(mode);
  const commands = useThreadCommands(threadId, known);
  const suggestions = slashSuggestions(draft.text, commands);
  const open = suggestions.length > 0 && !dismissed;

  const send = (chosen: SendMode) => {
    if (isBlank(draft)) {
      return;
    }
    const message = expandPastes(draft.text, draft.pastes).trim();
    const images = draft.images;
    const queued = behavior(chosen);
    const sent = draft;
    history.end();
    setDraft(EMPTY_DRAFT);
    call("thread.prompt", {
      threadId,
      text: message,
      ...(images.length === 0 ? {} : { images: [...images] }),
      ...(queued === undefined ? {} : { behavior: queued }),
    }).then(
      () => {
        if (queued !== undefined) {
          announce(strings.chat.announce.queued);
        }
      },
      () => setDraft((previous) => restoreFailedPrompt(previous, sent)),
    );
  };
  const abort = useCallback(() => {
    call("thread.abort", { threadId }).then(
      (result) => {
        const { steering, followUp } = restoredTexts(result);
        setDraft((previous) => ({ ...previous, text: restoreDraft(previous.text, steering, followUp) }));
        announce(strings.chat.announce.aborted);
      },
      () => announce(strings.chat.announce.abortFailed),
    );
  }, [call, threadId, setDraft, announce]);
  // The action bar gets one stable function that sends the latest draft.
  const latestSend = useRef(send);
  latestSend.current = send;
  const sendLatest = useCallback((chosen: SendMode) => latestSend.current(chosen), []);
  const leave = () => {
    inputRef.current?.blur();
    document.querySelector<HTMLElement>(`[${SCOPE_ATTRIBUTE}="transcript"]`)?.focus();
  };
  const pick = (suggestion: Suggestion) => {
    setDraft((previous) => ({ ...previous, text: complete(suggestion) }));
    setDismissed(false);
  };
  const attach = (files: readonly File[]) => {
    void attachments(files).then((images) => {
      if (images.length > 0) {
        setDraft((previous) => ({ ...previous, images: [...previous.images, ...images] }));
      }
    });
  };

  /** `Ctrl-C` (UI-063): clears a non-empty draft, kept for `u`; aborts an empty one (or on a second press within
   * {@link INTERRUPT_WINDOW_MS}) without putting the queued texts back. */
  const interrupt = () => {
    const now = clock.now();
    const current = host.window.get(draftKey(threadId));
    if (interruptAction(current, now, lastInterrupt.current) === "clear") {
      lastInterrupt.current = now;
      setCleared(current);
      setDraft(EMPTY_DRAFT);
      setInterruptHint(true);
      announce(strings.chat.announce.draftCleared);
      return;
    }
    lastInterrupt.current = undefined;
    setInterruptHint(false);
    if (abortable) {
      call("thread.abort", { threadId }).then(
        () => announce(strings.chat.announce.aborted),
        () => announce(strings.chat.announce.abortFailed),
      );
    }
  };
  useEffect(() => {
    if (!interruptHint) {
      return undefined;
    }
    return clock.every(INTERRUPT_WINDOW_MS / 4, () => {
      const since = lastInterrupt.current;
      if (since === undefined || clock.now() - since > INTERRUPT_WINDOW_MS) {
        setInterruptHint(false);
      }
    });
  }, [clock, interruptHint]);

  useCommand({ id: "composer.focus", title: text.focus, run: () => inputRef.current?.focus() });
  useCommand({ id: "thread.abort", title: text.abortCommand, enabled: () => abortable, run: abort });
  useCommand({
    id: "focus.leave",
    title: text.leave,
    scope: "composer",
    run: () => (abortable ? abort() : leave()),
  });
  useCommand({ id: "composer.interrupt", title: text.interruptCommand, scope: "composer", run: interrupt });
  useCommand({
    id: "composer.restore",
    title: text.restoreCommand,
    enabled: () => cleared !== undefined,
    run: () => {
      if (cleared !== undefined) {
        setDraft((previous) => restoreCleared(previous, cleared));
        setCleared(undefined);
      }
    },
  });

  const pastes = usePastes(draft, setDraft, inputRef);
  const history = usePromptHistory(threadId, draft, setDraft, inputRef);
  // Captured before React Aria's Autocomplete: while no suggestion list is open the text area keeps its own keys
  // (arrows move the caret), and the composer's bindings apply. Chords other than the send ones (Ctrl-C, Alt-a, …)
  // go on to the keymap (UI-095).
  const onKeyDownCapture = (event: KeyboardEvent<HTMLElement>) => {
    if (event.target !== inputRef.current || event.nativeEvent.isComposing) {
      return;
    }
    const modifiers = modifiersOf(event);
    pastes.onKeyDown(event, modifiers);
    setSteer(steerModifier(modifiers));
    if (open && !(event.key === "Enter" && !inputRef.current.hasAttribute("aria-activedescendant"))) {
      if (event.key === "Tab" && !event.shiftKey) {
        event.preventDefault();
        event.stopPropagation();
        const active = document.getElementById(inputRef.current.getAttribute("aria-activedescendant") ?? "");
        const name = active?.getAttribute("data-key");
        const first = suggestions.find((suggestion) => suggestion.name === name) ?? suggestions[0];
        if (first !== undefined) {
          pick(first);
        }
      } else if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        setDismissed(true);
      }
      return;
    }
    // Up on the first line, Down on the last line: the prompt history (UI-361, UI-362).
    if (history.onKey(event)) {
      event.stopPropagation();
      return;
    }
    if ((modifiers.ctrl || modifiers.alt || modifiers.meta) && event.key !== "Enter") {
      return;
    }
    event.stopPropagation();
    const action = keyAction(event.key, modifiers, { busy, abortable, hasSuggestions: false, repeat: event.repeat });
    if (action.type === "none") {
      return;
    }
    event.preventDefault();
    if (action.type === "send") {
      send(action.mode);
    } else if (action.type === "newline") {
      insertLineBreak(inputRef.current, pastes.onChange);
    } else if (action.type === "abort") {
      abort();
    } else if (action.type === "leave") {
      leave();
    }
  };
  const onPaste = (event: ClipboardEvent<HTMLTextAreaElement>) => {
    const files = [...event.clipboardData.files].filter((file) => file.type.startsWith("image/"));
    if (files.length > 0) {
      event.preventDefault();
      attach(files);
      return;
    }
    pastes.onPaste(event);
  };
  const onDrop = (event: DragEvent<HTMLFormElement>) => {
    event.preventDefault();
    attach([...event.dataTransfer.files]);
  };

  return (
    <form
      className="omfp-composer"
      data-testid="composer"
      data-busy={busy}
      {...{ [SCOPE_ATTRIBUTE]: "composer" }}
      onSubmit={(event) => {
        event.preventDefault();
        send(mode);
      }}
      onDragOver={(event) => event.preventDefault()}
      onDrop={onDrop}
      onKeyDownCapture={onKeyDownCapture}
      onKeyUpCapture={(event) => setSteer(steerModifier(modifiersOf(event)))}
    >
      <KitComposer mode={insert ? "insert" : "normal"}>
        {draft.images.length > 0 && <Attachments draft={draft} setDraft={setDraft} />}
        {draft.pastes.length > 0 && <PasteChips draft={draft} setDraft={setDraft} inputRef={inputRef} />}
        <Autocomplete
          inputValue={draft.text}
          onInputChange={(value) => {
            pastes.onChange(value);
            // Typing ends a history walk; the recalled text stays as the draft (UI-362).
            history.end();
            setDismissed(false);
            // Typing ends a `Ctrl-C` sequence: the next press clears what was typed rather than aborting (UI-063).
            lastInterrupt.current = undefined;
            setInterruptHint(false);
          }}
        >
          <TextArea
            className="omfp-compose__field"
            label={text.label}
            hideLabel
            rows={2}
            placeholder={busy ? text.placeholderBusy : text.placeholderIdle}
            inputRef={inputRef}
            onFocus={() => setInsert(true)}
            onBlur={() => {
              setSteer(false);
              setInsert(false);
            }}
            inputProps={{
              ...testId("composer-input"),
              "aria-keyshortcuts": `Enter Shift+Enter Alt+Enter Escape Tab ${ariaKeys(rawPasteKeys())}`,
              onPaste,
            }}
          />
          <Popover
            className="omfp-composer__menu"
            triggerRef={inputRef}
            isOpen={open}
            isNonModal
            placement="top start"
            onOpenChange={(isOpen) => setDismissed(!isOpen)}
          >
            <Menu
              aria-label={text.commands}
              data-testid="slash-suggestions"
              items={suggestions.map((suggestion) => ({ id: suggestion.name, suggestion }))}
              onAction={(key) => {
                const chosen = suggestions.find((suggestion) => suggestion.name === key);
                if (chosen !== undefined) {
                  pick(chosen);
                }
              }}
            >
              {({ suggestion }) => (
                <MenuItem id={suggestion.name} textValue={`/${suggestion.name}`} data-testid="slash-suggestion">
                  <span className="omfp-composer__command">/{suggestion.name}</span>
                  {suggestion.hint !== undefined && <span className="omfp-composer__hint"> {suggestion.hint}</span>}
                  {suggestion.description !== undefined && (
                    <span className="omfp-composer__description"> {suggestion.description}</span>
                  )}
                </MenuItem>
              )}
            </Menu>
          </Popover>
        </Autocomplete>
        <ComposerActions
          threadId={threadId}
          abortable={abortable}
          interruptHint={interruptHint}
          mode={mode}
          other={other}
          onAbort={abort}
          onSend={sendLatest}
        />
      </KitComposer>
    </form>
  );
});

/** The hints and buttons under the input; memoized with stable callbacks, so typing does not render them (UI-197). */
const ComposerActions = memo(function ComposerActions({
  threadId,
  abortable,
  interruptHint,
  mode,
  other,
  onAbort,
  onSend,
}: {
  threadId: string;
  abortable: boolean;
  interruptHint: boolean;
  mode: SendMode;
  other: SendMode | undefined;
  onAbort: () => void;
  onSend: (mode: SendMode) => void;
}) {
  return (
    <ComposerBar>
      <ComposerHint>
        <Kbd>{keyLabel("Alt-Enter")}</Kbd> {text.hint.newline}
      </ComposerHint>
      <ComposerHint>
        <Kbd>{keyLabel("Esc")}</Kbd> {abortable ? text.hint.abort : text.hint.leave}
      </ComposerHint>
      {interruptHint && abortable && (
        <ComposerHint className="omfp-composer__interrupt" role="status" data-testid="interrupt-hint">
          <Kbd>{keyLabel("Ctrl-c")}</Kbd> {text.interruptHint}
        </ComposerHint>
      )}
      <span className="omfp-composer__spacer" />
      <ThreadMicButton threadId={threadId} />
      {abortable && (
        <Button variant="danger" size="sm" icon={<Icon name="square" />} data-testid="abort" onPress={onAbort}>
          {text.abort}
        </Button>
      )}
      {other !== undefined && (
        <Button
          size="sm"
          kbd={sendKey(other)}
          data-testid="send-alternative"
          data-mode={other}
          onPress={() => onSend(other)}
        >
          {text.send[other]}
        </Button>
      )}
      <Button
        type="submit"
        variant="primary"
        size="sm"
        icon={<Icon name="arrow-up" />}
        kbd={sendKey(mode)}
        data-testid="send"
        data-mode={mode}
      >
        {text.send[mode]}
      </Button>
    </ComposerBar>
  );
});

/** The mic button follows the thread's state itself, so thread events render it alone. */
function ThreadMicButton({ threadId }: { threadId: string }) {
  const view = useThread(threadId);
  return view === undefined ? null : <MicButton threadId={threadId} state={view.state} />;
}
