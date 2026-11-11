/** One transcript row: a message with its blocks (text, thinking, images, tool cards, records) or a notice row. */

import {
  Code,
  Disclosure,
  ExitCode,
  Markdown,
  Message,
  StreamCaret,
  Thinking,
  ToolCard,
  type ToolStatus,
} from "omfp/kit";
import type { MessageBlock as Block, MessageView, NoticeRow, MessageRole as Role, SystemRow } from "omfp/sdk";
import { type ToolCard as Card, expandedByDefault, formatDuration, strings, useOpenFile } from "omfp/sdk";
import { type ReactNode, useCallback } from "react";
import { useExpanded } from "./state.ts";
import { fileTarget, ToolBody, ToolSubject } from "./tool-cards.tsx";

const text = strings.chat;

/** One message: name line, blocks, error (UI-171). `cardKey` prefixes the keys of its collapsible blocks. While it
 * streams, a block caret follows the last character (UI-147). */
export function MessageRow({ cardKey, view, streaming }: { cardKey: string; view: MessageView; streaming: boolean }) {
  const name = (
    <>
      {view.label}
      {view.steering && (
        <span className="omfp-message__tag" data-testid="steering-mark">
          {text.steeringMark}
        </span>
      )}
    </>
  );
  const blocks = view.blocks.map((block, index) => (
    // Blocks only grow at the end while streaming, so the index is a stable key.
    // biome-ignore lint/suspicious/noArrayIndexKey: see above
    <BlockView key={index} cardKey={`${cardKey}/${index}`} role={view.role} block={block} />
  ));
  const error = view.error === undefined ? undefined : <span data-testid="message-error">{view.error}</span>;
  const caret = streaming && view.blocks.at(-1)?.type !== "text" ? <StreamCaret /> : null;
  const common = {
    className: "omfp-message",
    "data-testid": "message",
    "data-role": view.role,
    "data-streaming": streaming,
    "aria-busy": streaming,
  };
  if (view.role === "record") {
    return (
      <article {...common}>
        <div className="omfp-msg__name">{name}</div>
        {blocks}
        {error !== undefined && <p className="omfp-msg__error">{error}</p>}
      </article>
    );
  }
  return (
    <Message {...common} author={view.role} name={name} error={error}>
      {blocks}
      {caret}
    </Message>
  );
}

function BlockView({ cardKey, role, block }: { cardKey: string; role: Role; block: Block }) {
  const openFile = useOpenFile();
  // Paths in the assistant's text open the file (UI-116).
  const openPath = useCallback(
    (reference: { path: string; line: number | undefined }) => openFile(reference.path, { line: reference.line }),
    [openFile],
  );
  switch (block.type) {
    case "text":
      return role === "assistant" ? (
        <Markdown text={block.text} data-testid="markdown" onOpenPath={openPath} />
      ) : (
        <div className="omfp-message__plain omfp-prose" data-testid="plain-text">
          {block.text}
        </div>
      );
    case "thinking":
      return <ThinkingBlock cardKey={cardKey} text={block.text} />;
    case "image":
      return (
        <img
          className="omfp-message__image"
          data-testid="message-image"
          src={`data:${block.mime};base64,${block.data}`}
          alt={text.attachedImage}
        />
      );
    case "tool":
      return <ToolCardView cardKey={cardKey} card={block.card} />;
    case "record":
      return <RecordBlock cardKey={cardKey} summary={block.summary} json={block.json} />;
  }
}

/** Thinking, collapsed by default (UI-041, UI-171). */
function ThinkingBlock({ cardKey, text: thinking }: { cardKey: string; text: string }) {
  const [expanded, setExpanded] = useExpanded(cardKey, false);
  return (
    <Thinking
      className="omfp-thinking"
      data-testid="thinking"
      label={text.thinking}
      expanded={expanded}
      onExpandedChange={setExpanded}
    >
      {thinking}
    </Thinking>
  );
}

function RecordBlock({ cardKey, summary, json }: { cardKey: string; summary: string; json: string }) {
  const [expanded, setExpanded] = useExpanded(cardKey, false);
  return (
    <div className="omfp-record" data-testid="record" data-expanded={expanded}>
      <Disclosure title={summary} expanded={expanded} onExpandedChange={setExpanded} level={4}>
        <Code code={json} block data-testid="record-json" />
      </Disclosure>
    </div>
  );
}

/** The status mark of a call (UI-163): composing, pending and running calls spin. */
function markStatus(status: Card["status"]): ToolStatus {
  switch (status) {
    case "succeeded":
      return "done";
    case "failed":
      return "failed";
    default:
      return "running";
  }
}

/** A tool card (UI-050, UI-163): collapsible, failed cards distinct and expanded by default (UI-052); tools with a
 * recorded shape show their specialized body, others the fallback's arguments and result. */
export function ToolCardView({ cardKey, card }: { cardKey: string; card: Card }) {
  const [expanded, setExpanded] = useExpanded(cardKey, expandedByDefault(card));
  // `g f` on the focused card opens its file (UI-116).
  const target = fileTarget(card.view);
  const exitCode = card.view.kind === "bash" ? card.view.exitCode : undefined;
  return (
    <div
      data-omfp-path={target?.path}
      data-omfp-line={target?.open.line}
      data-omfp-diff={target?.open.diff === true ? "" : undefined}
      className="omfp-tool-card"
      data-testid="tool-card"
      data-tool={card.name}
      data-call={card.toolCallId}
      data-view={card.view.kind}
      data-status={card.status}
    >
      <ToolCard
        aria-label={text.tool.label(card.name)}
        status={markStatus(card.status)}
        name={card.name}
        headTestId="toggle"
        expanded={expanded}
        onExpandedChange={setExpanded}
        arg={
          <>
            <ToolSubject view={card.view} />
            <span className="omfp-tool-card__summary" data-testid="tool-summary">
              {card.summary}
            </span>
          </>
        }
        meta={<ToolMeta card={card} exitCode={exitCode} />}
      >
        <ToolBody card={card} />
      </ToolCard>
    </div>
  );
}

/** Right-aligned metadata of a card's head: the status word for assistive tech, duration, exit code. */
function ToolMeta({ card, exitCode }: { card: Card; exitCode: number | undefined }): ReactNode {
  return (
    <>
      <span className="omfp-visually-hidden" data-testid="tool-status">
        {text.tool.status[card.status]}
      </span>
      {card.durationMs !== undefined && <span data-testid="tool-duration">{formatDuration(card.durationMs)}</span>}
      {exitCode !== undefined && <ExitCode code={exitCode} data-testid="tool-exit" />}
    </>
  );
}

/** An inline notice row (UI-043); unknown frames expand to their JSON. */
export function NoticeRowView({ cardKey, row }: { cardKey: string; row: NoticeRow }) {
  const [expanded, setExpanded] = useExpanded(cardKey, false);
  return (
    <div className="omfp-notice" data-testid="notice" data-kind={row.kind} data-level={row.level}>
      <span className="omfp-notice__text">{row.text}</span>
      {row.json !== undefined && (
        <Disclosure title={text.details} expanded={expanded} onExpandedChange={setExpanded} level={4}>
          <Code code={row.json} block data-testid="notice-json" />
        </Disclosure>
      )}
    </div>
  );
}

/** omp's own bookkeeping (UI-043): one muted `system` row, expandable to the whole text when there is more. */
export function SystemRowView({ cardKey, row }: { cardKey: string; row: SystemRow }) {
  const [expanded, setExpanded] = useExpanded(cardKey, false);
  const summary = (
    <span className="omfp-system__summary" data-testid="system-summary" title={row.summary}>
      {row.summary}
    </span>
  );
  return (
    <div className="omfp-system" data-testid="system-record" data-expanded={expanded}>
      <span className="omfp-system__label" data-testid="system-label">
        {text.system}
      </span>
      {row.text === undefined ? (
        summary
      ) : (
        <Disclosure title={summary} expanded={expanded} onExpandedChange={setExpanded} level={4}>
          <div className="omfp-system__text" data-testid="system-text">
            {row.text}
          </div>
        </Disclosure>
      )}
    </div>
  );
}
