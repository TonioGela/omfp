/**
 * The shell's pieces (UI-167..UI-170): the mode line with its mode pill, the command line and its completion menu,
 * the brand, workspace sections and the left column's rows. They are presentational: the app wires keys, focus and
 * data; forced state classes (`is-hover`, `is-focus`) render hover and focus statically (UI-173).
 */
import type { HTMLAttributes, ReactNode } from "react";
import { Button as AriaButton, type ButtonProps as AriaButtonProps } from "react-aria-components";
import { IconButton } from "./button.tsx";
import { cx, withClass } from "./class.ts";
import { Icon, type IconName } from "./icons.tsx";
import { Kbd } from "./kbd.tsx";
import { StateMark, type ThreadState } from "./marks.tsx";

/** The keymap modes (UI-098). */
export type Mode = "normal" | "insert" | "command";

export interface ModePillProps extends HTMLAttributes<HTMLSpanElement> {
  mode: Mode;
  /** The pill's text (the mode in capitals by default). */
  children?: ReactNode;
}

/** `NORMAL`, `INSERT` or `COMMAND` (UI-167); it rises in when the mode changes at motion `full`. */
export function ModePill({ mode, className, children, ...props }: ModePillProps) {
  return (
    <span key={mode} {...props} className={cx("omfp-mode-pill", className)} data-mode={mode}>
      {children ?? mode.toUpperCase()}
    </span>
  );
}

export interface ModeLineProps extends HTMLAttributes<HTMLDivElement> {
  mode: Mode;
  /** The mode pill's text. */
  modeLabel?: ReactNode;
  /** A {@link ModeNotice} at the right end, outside the segments that drop when narrow. */
  notice?: ReactNode;
  /** Segments ({@link ModeSegment}), or the command line in command mode. */
  children?: ReactNode;
}

/**
 * The bottom bar (UI-167): a gradient hairline on top, the mode pill, then segments separated by `·`; segments that do
 * not fit drop from the right; a notice (UI-193) keeps the right end.
 */
export function ModeLine({ mode, modeLabel, notice, className, children, ...props }: ModeLineProps) {
  return (
    <div {...props} className={cx("omfp-modeline", className)} data-mode={mode}>
      <ModePill mode={mode}>{modeLabel}</ModePill>
      <div className="omfp-modeline__segments">{children}</div>
      {notice}
    </div>
  );
}

export interface ModeSegmentProps extends HTMLAttributes<HTMLSpanElement> {
  /** `title`: the thread title (on-surface, ellipsis); `message` / `error`: a transient message. */
  kind?: "plain" | "title" | "message" | "error";
}

/** One segment of the mode line. */
export function ModeSegment({ kind = "plain", className, ...props }: ModeSegmentProps) {
  return <span {...props} className={cx("omfp-modeline__segment", className)} data-kind={kind} />;
}

/** The tone of a mode-line notice: a thread awaiting the user, a failure, a finished turn, anything else. */
export type NoticeTone = "attention" | "error" | "ok" | "info";

export interface ModeNoticeProps extends Omit<AriaButtonProps, "children"> {
  tone: NoticeTone;
  /** What it is about (a thread title), on-surface with an ellipsis. */
  title: ReactNode;
  /** What happened (`needs you`), in the tone's color. */
  body: ReactNode;
  /** Notices behind this one, shown as `+n`. */
  more?: number;
  /** The key that opens it, as a key chip (`g n`). */
  kbd?: string;
}

/**
 * A notice at the right end of the mode line (UI-193), like vim's message area: `◆`, the title and what happened in
 * the tone's color, `+n` more and the key chip; pressing it opens what it is about. It pops in, and its `◆` pulses
 * at motion `full`.
 */
export function ModeNotice({ tone, title, body, more = 0, kbd, className, ...props }: ModeNoticeProps) {
  return (
    <AriaButton {...props} className={withClass("omfp-modeline__notice", className)} data-tone={tone}>
      <span className="omfp-modeline__notice-mark" aria-hidden="true">
        ◆
      </span>
      <span className="omfp-modeline__notice-title">{title}</span>
      <span className="omfp-modeline__notice-body">{body}</span>
      {more > 0 && <span className="omfp-modeline__notice-more">+{more}</span>}
      {kbd !== undefined && <Kbd aria-hidden="true">{kbd}</Kbd>}
    </AriaButton>
  );
}

export type CommandLineProps = HTMLAttributes<HTMLSpanElement>;

/**
 * The `:` line (UI-168): a gradient `:` prompt, then the input (give it the `omfp-cmd__input` class) or static
 * text.
 */
export function CommandLine({ className, children, ...props }: CommandLineProps) {
  return (
    <span {...props} className={cx("omfp-cmd", className)}>
      <span className="omfp-cmd__prompt" aria-hidden="true">
        :
      </span>
      {children}
    </span>
  );
}

export interface CommandMenuProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  /** The title after `╭─` (`commands` by default). */
  title?: ReactNode;
  /** The key row under the items (`tab`, `enter`, `esc` chips). */
  footer?: ReactNode;
  /** {@link CommandItem}s. */
  children?: ReactNode;
  /** Props of the list element (its `id`, `role`, `aria-*`). */
  listProps?: HTMLAttributes<HTMLDivElement>;
}

/** The command line's completions, a popover above it titled `╭─ commands` (UI-168). */
export function CommandMenu({
  title = "commands",
  footer,
  className,
  children,
  listProps,
  ...props
}: CommandMenuProps) {
  return (
    <div {...props} className={cx("omfp-popover omfp-cmdmenu", className)}>
      <div className="omfp-cmdmenu__title" aria-hidden="true">
        ╭─ {title}
      </div>
      <div role="listbox" {...listProps} className={cx("omfp-cmdmenu__list", listProps?.className)}>
        {children}
      </div>
      {footer !== undefined && <div className="omfp-cmdmenu__foot">{footer}</div>}
    </div>
  );
}

export interface CommandItemProps extends HTMLAttributes<HTMLDivElement> {
  /** The command, without its colon. */
  command: string;
  /** How many leading characters of `command` the typed text matched. */
  matched?: number;
  description?: ReactNode;
  selected?: boolean;
}

/** One completion: `▸` and the soft gradient when selected, the matched part in tertiary bold. */
export function CommandItem({
  command,
  matched = 0,
  description,
  selected = false,
  className,
  ...props
}: CommandItemProps) {
  return (
    <div
      role="option"
      aria-selected={selected}
      tabIndex={-1}
      {...props}
      className={cx("omfp-cmdmenu__item", className)}
      data-selected={selected || undefined}
    >
      <span className="omfp-cmdmenu__command">
        {matched > 0 && <b>{command.slice(0, matched)}</b>}
        {command.slice(matched)}
      </span>
      {description !== undefined && <span className="omfp-cmdmenu__description">{description}</span>}
    </div>
  );
}

export interface BrandProps extends HTMLAttributes<HTMLDivElement> {
  /** The daemon host, after `@`. */
  host?: string;
}

/** `◆ omfp` in flowing gradient text and `@host` (UI-170). */
export function Brand({ host, className, ...props }: BrandProps) {
  return (
    <div {...props} className={cx("omfp-brand", className)}>
      <span className="omfp-brand__mark omfp-grad-text">◆ omfp</span>
      {host !== undefined && <span className="omfp-brand__host">@{host}</span>}
    </div>
  );
}

export interface WorkspaceSectionProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  name: ReactNode;
  path?: ReactNode;
  count?: ReactNode;
  expanded: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  /** The workspace of the open thread: its name flows in gradient text. */
  current?: boolean;
  /** Class names of the head (forced states in the gallery). */
  headClassName?: string;
  /** `data-testid` of the head (`workspace-head` by default). */
  headTestId?: string;
  /** Its thread rows, action rows, or an empty hint row. */
  children?: ReactNode;
}

/** A workspace in the left column (UI-170): a `▾`/`▸` head with name, path and count, then its rows. */
export function WorkspaceSection({
  name,
  path,
  count,
  expanded,
  onExpandedChange,
  current = false,
  headClassName,
  headTestId = "workspace-head",
  className,
  children,
  ...props
}: WorkspaceSectionProps) {
  return (
    <section
      {...props}
      className={cx("omfp-wsection", className)}
      data-expanded={expanded}
      data-current={current || undefined}
    >
      <AriaButton
        className={cx("omfp-wsection__head", headClassName)}
        aria-expanded={expanded}
        onPress={() => onExpandedChange?.(!expanded)}
        data-testid={headTestId}
      >
        <span className="omfp-wsection__glyph" aria-hidden="true">
          {expanded ? "▾" : "▸"}
        </span>
        <span className={cx("omfp-wsection__name", current && "omfp-grad-text")}>{name}</span>
        {path !== undefined && <span className="omfp-wsection__path">{path}</span>}
        {count !== undefined && <span className="omfp-wsection__count">{count}</span>}
      </AriaButton>
      {expanded && <div className="omfp-wsection__rows">{children}</div>}
    </section>
  );
}

export interface ThreadRowProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  state: ThreadState;
  title: ReactNode;
  /** Relative time, replaced by the delete `×` on hover and focus. */
  meta?: ReactNode;
  selected?: boolean;
  /** Just started awaiting or failed: flashes and bounces at motion `full` (UI-146). */
  fresh?: boolean;
  /** Shows the delete `×`. */
  onDelete?: () => void;
  deleteLabel?: string;
}

/** A thread in the left column (UI-169): state mark, title, time or delete; selected shows the gradient bar. */
export function ThreadRow({
  state,
  title,
  meta,
  selected = false,
  fresh = false,
  onDelete,
  deleteLabel = "Delete thread",
  className,
  ...props
}: ThreadRowProps) {
  return (
    <div
      {...props}
      className={cx("omfp-row omfp-thread", className)}
      data-state={state}
      data-selected={selected || undefined}
      data-fresh={fresh || undefined}
    >
      <StateMark state={state} />
      <span className="omfp-row__title">{title}</span>
      {meta !== undefined && <span className="omfp-row__meta">{meta}</span>}
      {onDelete !== undefined && (
        <IconButton
          className="omfp-row__delete"
          label={deleteLabel}
          icon={<Icon name="x" />}
          onPress={onDelete}
          data-testid="row-delete"
        />
      )}
    </div>
  );
}

export interface ActionRowProps extends AriaButtonProps {
  /** `action`: a "+ thread"-like row with a primary icon; `archived`: a muted, indented row. */
  kind?: "action" | "archived";
  icon?: IconName;
  children?: ReactNode;
}

/** An action or archived row sharing the thread row's shape (UI-169). */
export function ActionRow({ kind = "action", icon, className, children, ...props }: ActionRowProps) {
  return (
    <AriaButton {...props} className={withClass("omfp-row omfp-action-row", className)} data-kind={kind}>
      {icon !== undefined && <Icon name={icon} />}
      <span className="omfp-row__title">{children}</span>
    </AriaButton>
  );
}

export type HintRowProps = HTMLAttributes<HTMLDivElement>;

/** The empty-state hint of an empty workspace section (UI-170). */
export function HintRow({ className, ...props }: HintRowProps) {
  return <div {...props} className={cx("omfp-row omfp-hint-row", className)} />;
}
