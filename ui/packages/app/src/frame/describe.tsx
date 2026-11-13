/**
 * `:describe-key` and `:describe-command <name>` (UI-264, UI-265, experimental): a modal like the help panel. For
 * `:describe-key` it first takes the keys to describe (they run nothing: inside a dialog no binding applies), then shows
 * what they run; Escape or a click outside closes it and the focus goes back where it was.
 */

import { HelpDialog, HelpSection, Kbd } from "@omfp/kit";
import { type KeyboardEvent, type ReactNode, useEffect, useReducer, useRef, useState } from "react";
import { type Binding, describeKeys, keyLabel, keyToken } from "../keymap/keymap.ts";
import { useCommand, useHost } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import { type DescribeSection, describeCommand, describeKey } from "./describe.ts";

const text = strings.init.describe;

type Describing =
  /** Taking keys: those typed so far. */
  | { readonly kind: "keys"; readonly typed: readonly string[] }
  | { readonly kind: "key"; readonly keys: string }
  | { readonly kind: "command"; readonly name: string };

export function Describe({ bindings }: { readonly bindings: readonly Binding[] }): ReactNode {
  const [describing, setDescribing] = useState<Describing | undefined>(undefined);
  const previous = useRef<Element | null>(null);
  const restore = useRef(false);
  const open = (next: Describing) => {
    previous.current = document.activeElement;
    setDescribing(next);
  };
  useCommand({
    id: "help.describeKey",
    title: text.key,
    aliases: ["describe-key"],
    run: () => open({ kind: "keys", typed: [] }),
  });
  useCommand({
    id: "help.describeCommand",
    title: text.command,
    aliases: ["describe-command"],
    run: (args) => open({ kind: "command", name: args.join(" ").trim() }),
  });
  // After the modal is gone, so its focus containment does not pull the focus back in.
  useEffect(() => {
    const back = previous.current;
    if (describing === undefined && restore.current) {
      restore.current = false;
      if (back instanceof HTMLElement && back.isConnected) {
        back.focus();
      }
    }
  }, [describing]);
  if (describing === undefined) {
    return null;
  }
  const close = () => {
    restore.current = true;
    setDescribing(undefined);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    const token = keyToken(event.nativeEvent);
    if (describing.kind !== "keys" || token === undefined || token === "Esc" || event.nativeEvent.isComposing) {
      return;
    }
    event.preventDefault();
    const keys = [...describing.typed, token];
    const described = describeKeys(bindings, keys.join(" "));
    setDescribing(described.kind === "pending" ? { kind: "keys", typed: keys } : { kind: "key", keys: described.keys });
  };
  return <DescribePanel describing={describing} bindings={bindings} close={close} onKeyDown={onKeyDown} />;
}

function DescribePanel({
  describing,
  bindings,
  close,
  onKeyDown,
}: {
  describing: Describing;
  bindings: readonly Binding[];
  close: () => void;
  onKeyDown: (event: KeyboardEvent<HTMLElement>) => void;
}): ReactNode {
  const { commands } = useHost();
  // Commands registered or removed while it is open show at once.
  const [, changed] = useReducer((count: number) => count + 1, 0);
  useEffect(() => commands.subscribe(changed), [commands]);
  const sections: DescribeSection[] =
    describing.kind === "key"
      ? describeKey(describing.keys, bindings, commands.list())
      : describing.kind === "command"
        ? describeCommand(describing.name, bindings, commands.list())
        : [];
  const prompt =
    describing.kind === "keys"
      ? describing.typed.length === 0
        ? text.press
        : text.pending(keyLabel(describing.typed.join(" ")))
      : describing.kind === "key"
        ? keyLabel(describing.keys)
        : `:describe-command ${describing.name}`;
  // The prompt holds the focus, so the keys to describe reach the dialog and Escape closes it.
  const promptRef = useRef<HTMLParagraphElement>(null);
  useEffect(() => {
    promptRef.current?.focus();
  }, []);
  return (
    <HelpDialog
      title={text.title}
      label={text.label}
      data-testid="describe"
      data-state={describing.kind}
      onClose={close}
      onKeyDown={onKeyDown}
      end={
        <>
          <Kbd>{keyLabel("Esc")}</Kbd>
          <span>{text.close}</span>
        </>
      }
      filter={
        <p ref={promptRef} tabIndex={-1} className="omfp-help__note" data-testid="describe-prompt">
          {prompt}
        </p>
      }
    >
      {sections.map((section) => (
        <HelpSection key={section.id} title={section.title} data-section={section.id} data-testid="describe-section">
          {section.fields.map((field) => (
            <div key={`${field.label}\u0000${field.value}`} className="omfp-help__row" data-testid="describe-field">
              <dt className="omfp-help__keys">{field.label}</dt>
              <dd className="omfp-help__description">
                {field.keys?.map((keys) => (
                  <Kbd key={keys}>{keys}</Kbd>
                ))}
                {field.keys !== undefined && field.value !== "" && " "}
                {field.value}
              </dd>
            </div>
          ))}
        </HelpSection>
      ))}
    </HelpDialog>
  );
}
