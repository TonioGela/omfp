/** One transcript row: a message with its blocks (text, thinking, images, tool cards, records) or a notice row. */

import { Card, Code, Disclosure, Markdown } from "@omfp/kit";
import { strings } from "../../strings/index.ts";
import type { NoticeRow } from "../../vm/notice.ts";
import type { SystemRow } from "../../vm/system.ts";
import { expandedByDefault, formatDuration, type ToolCard } from "../../vm/tool.ts";
import type { Block, MessageView, Role } from "../../vm/transcript.ts";
import { useExpanded } from "./state.ts";

const text = strings.chat;

/** One message: label, blocks, error. `cardKey` prefixes the keys of its collapsible blocks. */
export function MessageRow({ cardKey, view, streaming }: { cardKey: string; view: MessageView; streaming: boolean }) {
  return (
    <article
      className="omfp-message"
      data-testid="message"
      data-role={view.role}
      data-streaming={streaming}
      aria-busy={streaming}
    >
      <div className="omfp-message__label">
        {view.label}
        {view.steering && (
          <span className="omfp-message__tag" data-testid="steering-mark">
            {text.steeringMark}
          </span>
        )}
      </div>
      {view.blocks.map((block, index) => (
        // Blocks only grow at the end while streaming, so the index is a stable key.
        // biome-ignore lint/suspicious/noArrayIndexKey: see above
        <BlockView key={index} cardKey={`${cardKey}/${index}`} role={view.role} block={block} />
      ))}
      {view.error !== undefined && (
        <p className="omfp-message__error" data-testid="message-error">
          {view.error}
        </p>
      )}
    </article>
  );
}

function BlockView({ cardKey, role, block }: { cardKey: string; role: Role; block: Block }) {
  switch (block.type) {
    case "text":
      return role === "assistant" ? (
        <Markdown text={block.text} data-testid="markdown" />
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

/** Thinking, collapsed by default (UI-041). */
function ThinkingBlock({ cardKey, text: thinking }: { cardKey: string; text: string }) {
  const [expanded, setExpanded] = useExpanded(cardKey, false);
  return (
    <div className="omfp-thinking" data-testid="thinking" data-expanded={expanded}>
      <Disclosure title={text.thinking} expanded={expanded} onExpandedChange={setExpanded} level={4}>
        <div className="omfp-thinking__text omfp-prose">{thinking}</div>
      </Disclosure>
    </div>
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

/** The fallback tool card (UI-050): collapsible; failed cards distinct and expanded by default (UI-052). */
export function ToolCardView({ cardKey, card }: { cardKey: string; card: ToolCard }) {
  const [expanded, setExpanded] = useExpanded(cardKey, expandedByDefault(card));
  return (
    <Card
      className="omfp-tool-card"
      data-testid="tool-card"
      data-tool={card.name}
      data-status={card.status}
      aria-label={text.tool.label(card.name)}
      tone={card.status === "failed" ? "error" : "default"}
      collapsible={{ expanded, onExpandedChange: setExpanded }}
      title={
        <>
          <span className="omfp-tool-card__name">{card.name}</span>
          <span className="omfp-tool-card__summary" data-testid="tool-summary">
            {card.summary}
          </span>
          <span className="omfp-tool-card__status" data-testid="tool-status">
            {text.tool.status[card.status]}
          </span>
          {card.durationMs !== undefined && (
            <span className="omfp-tool-card__duration" data-testid="tool-duration">
              {formatDuration(card.durationMs)}
            </span>
          )}
        </>
      }
    >
      <div className="omfp-tool-card__section">
        <h4>{text.tool.arguments}</h4>
        <Code code={card.args} block data-testid="tool-args" />
      </div>
      {card.result !== undefined && (
        <div className="omfp-tool-card__section">
          <h4>{text.tool.result}</h4>
          <Code code={card.result} block data-testid="tool-result" />
        </div>
      )}
    </Card>
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
