/** Tool cards (UI-163), approval and ask cards with their resolved line (UI-164). */
import { type HTMLAttributes, type ReactNode, type Ref, useId } from "react";
import { Button as AriaButton } from "react-aria-components";
import { cx } from "./class.ts";
import { Icon } from "./icons.tsx";
import { StateMark } from "./marks.tsx";

/** Where a tool call is. */
export type ToolStatus = "running" | "done" | "failed";

export interface ToolCardProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  status: ToolStatus;
  /** The tool's name (`bash`, `edit`, …). */
  name: ReactNode;
  /** The main argument (a command, a path), cut with an ellipsis. */
  arg?: ReactNode;
  /** Right-aligned metadata: duration, exit code, line counts. */
  meta?: ReactNode;
  /** Whether the body shows; the head toggles it when there is a body. */
  expanded?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  /** `data-testid` of the head (`tool-head` by default). */
  headTestId?: string;
  /** The body: a tool renderer (output, diff, todo list). Without one the head is not clickable. */
  children?: ReactNode;
}

/**
 * A tool call (UI-163): a head with the status mark (`●` ok or error, the braille spinner while running), the name,
 * the main argument and metadata; the head toggles the body (no chevron). Running cards shimmer and breathe at motion
 * `full`.
 */
export function ToolCard({
  status,
  name,
  arg,
  meta,
  expanded = false,
  onExpandedChange,
  headTestId = "tool-head",
  className,
  children,
  ...props
}: ToolCardProps) {
  const bodyId = useId();
  const hasBody = children !== undefined && children !== null && children !== false;
  const head = (
    <>
      <StateMark state={status} />
      <span className="omfp-tool__name">{name}</span>
      {arg !== undefined && <span className="omfp-tool__arg">{arg}</span>}
      {meta !== undefined && <span className="omfp-tool__meta">{meta}</span>}
    </>
  );
  return (
    <section {...props} className={cx("omfp-tool", className)} data-status={status}>
      {hasBody ? (
        <AriaButton
          className="omfp-tool__head"
          aria-expanded={expanded}
          aria-controls={bodyId}
          onPress={() => onExpandedChange?.(!expanded)}
          data-testid={headTestId}
        >
          {head}
        </AriaButton>
      ) : (
        <div className="omfp-tool__head" data-testid={headTestId}>
          {head}
        </div>
      )}
      {hasBody && (
        <div id={bodyId} className="omfp-tool__body" hidden={!expanded}>
          {expanded && children}
        </div>
      )}
    </section>
  );
}

export interface TodoItem {
  readonly text: ReactNode;
  readonly status: "done" | "active" | "pending";
}

export interface TodoListProps extends Omit<HTMLAttributes<HTMLUListElement>, "children"> {
  items: readonly TodoItem[];
}

/** A tool card's todo list: a `●` per item, done in ok and struck through, active in primary (UI-163). */
export function TodoList({ items, className, ...props }: TodoListProps) {
  return (
    <ul {...props} className={cx("omfp-todo", className)}>
      {items.map((item, index) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: a todo list is shown as given
        <li key={index} className="omfp-todo__item" data-status={item.status}>
          <span className="omfp-todo__mark" aria-hidden="true">
            ●
          </span>
          <span className="omfp-todo__text">{item.text}</span>
        </li>
      ))}
    </ul>
  );
}

export interface InteractionCardProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  /** The question or the approval's title, after the `◆`. */
  title: ReactNode;
  /** Right-aligned metadata before the countdown, e.g. the step `2/3`. */
  meta?: ReactNode;
  /** Seconds left before the request times out; shown with a clock. */
  countdown?: string;
  /** The countdown is about to run out: it turns error. */
  low?: boolean;
  /** The card's `<section>` (React 19 passes it as a prop). */
  ref?: Ref<HTMLElement>;
  children?: ReactNode;
}

/** A pending approval or `ask` question (UI-164): a dashed warn card titled with `◆`. */
export function InteractionCard({
  title,
  meta,
  countdown,
  low = false,
  className,
  children,
  ...props
}: InteractionCardProps) {
  return (
    <section {...props} className={cx("omfp-request", className)}>
      <header className="omfp-request__head">
        <span className="omfp-request__title">
          <span className="omfp-request__glyph" aria-hidden="true">
            ◆{" "}
          </span>
          {title}
        </span>
        {(meta !== undefined || countdown !== undefined) && (
          <span className="omfp-request__meta">
            {meta}
            {countdown !== undefined && (
              <span className="omfp-request__countdown" data-low={low || undefined}>
                <Icon name="clock" />
                {countdown}
              </span>
            )}
          </span>
        )}
      </header>
      {children}
    </section>
  );
}

export interface ApprovalCommandProps extends Omit<HTMLAttributes<HTMLPreElement>, "children"> {
  command: string;
}

/** The command an approval asks about, after a muted `$` (UI-164). */
export function ApprovalCommand({ command, className, ...props }: ApprovalCommandProps) {
  return (
    <pre {...props} className={cx("omfp-request__code", className)}>
      <span className="omfp-request__prompt" aria-hidden="true">
        ${" "}
      </span>
      {command}
    </pre>
  );
}

export type InteractionActionsProps = HTMLAttributes<HTMLDivElement>;

/** The buttons row of an approval or ask card. */
export function InteractionActions({ className, ...props }: InteractionActionsProps) {
  return <div {...props} className={cx("omfp-request__actions", className)} />;
}

export interface OptionTextProps {
  label: ReactNode;
  description?: ReactNode;
}

/** An ask option's text: the label in the code family, the description in prose italic (UI-164). */
export function OptionText({ label, description }: OptionTextProps) {
  return (
    <span className="omfp-option-text">
      <span className="omfp-option-text__label">{label}</span>
      {description !== undefined && <span className="omfp-prose omfp-option-text__description">{description}</span>}
    </span>
  );
}

export interface ResolvedLineProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  /** `denied` shows an error x, anything else an ok check. */
  outcome: "approved" | "denied" | "answered";
  question: ReactNode;
  answer: ReactNode;
}

/** What replaces an answered card (UI-164): a check or x, the question cut with an ellipsis, the answer. */
export function ResolvedLine({ outcome, question, answer, className, ...props }: ResolvedLineProps) {
  return (
    <div {...props} className={cx("omfp-resolved", className)} data-outcome={outcome}>
      <Icon name={outcome === "denied" ? "x" : "check"} className="omfp-resolved__icon" />
      <span className="omfp-resolved__question">{question}</span>
      <span className="omfp-resolved__answer">{answer}</span>
    </div>
  );
}
