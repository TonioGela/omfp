/**
 * The window's messages (UI-521, UI-522), like Emacs's `*Messages*`: `:messages` (`help.messages`), the mode line's
 * messages button (UI-523) or the phone's Commands sheet opens a dialog listing the notices and toasts that went by,
 * newest first, each with its kind and time. Escape, a click outside or Close closes it, and the focus goes back.
 */

import { Button, Dialog, MessageLog, type MessageLogEntry, type MessageLogTone } from "@omfp/kit";
import { type ReactNode, useState } from "react";
import { keyLabel } from "../keymap/keymap.ts";
import { MESSAGES, type Message, type MessageTone, useCommand, useWindowState } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";

const text = strings.shell.messages;

/** The id of the command that opens the messages. */
export const MESSAGES_COMMAND = "help.messages";

const TONES: Readonly<Record<MessageTone, MessageLogTone>> = {
  attention: "attention",
  failed: "error",
  finished: "ok",
  info: "info",
  success: "ok",
  error: "error",
};

/** The log's lines of `messages` (oldest first), newest first; `time` formats a message's time. */
export function messageEntries(
  messages: readonly Message[],
  time: (at: number) => string = clockTime,
): MessageLogEntry[] {
  return [...messages].reverse().map((message) => ({
    key: message.id,
    tone: TONES[message.tone],
    kind: text.kinds[message.tone],
    time: time(message.at),
    dateTime: new Date(message.at).toISOString(),
    ...(message.title === undefined ? {} : { title: message.title }),
    text: message.text,
  }));
}

/** `14:03:12` in the browser's locale. */
function clockTime(at: number): string {
  return new Date(at).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

export function Messages(): ReactNode {
  const [open, setOpen] = useState(false);
  const [messages] = useWindowState(MESSAGES);
  useCommand({
    id: MESSAGES_COMMAND,
    title: strings.shell.commands.messages,
    aliases: ["messages"],
    run: () => setOpen(true),
  });
  return (
    <Dialog isOpen={open} onOpenChange={setOpen} isDismissable title={text.title} data-testid="messages-dialog">
      {({ close }) => (
        <>
          <MessageLog entries={messageEntries(messages)} emptyText={text.empty} aria-label={text.title} />
          <div className="omfp-dialog__actions">
            <Button variant="quiet" onPress={close} kbd={keyLabel("Escape")} data-testid="messages-close">
              {text.close}
            </Button>
          </div>
        </>
      )}
    </Dialog>
  );
}
