/**
 * The shell's pieces (UI-167..UI-170): the mode line with its mode pill, the command line and its completion menu,
 * the brand, workspace sections and the left column's rows, the help panel (UI-198) and the which-key panel (UI-320).
 * They are presentational: the app wires keys, focus and data; forced state classes (`is-hover`, `is-focus`) render
 * hover and focus statically (UI-173).
 */
import { type HTMLAttributes, type ReactNode, useId } from "react";
import {
  Button as AriaButton,
  type ButtonProps as AriaButtonProps,
  Dialog as AriaDialog,
  Modal,
  ModalOverlay,
} from "react-aria-components";
import { IconButton } from "./button.tsx";
import { BorderBox, BoxRule } from "./cards.tsx";
import { cx, withClass } from "./class.ts";
import { Icon, type IconName } from "./icons.tsx";
import { Kbd } from "./kbd.tsx";
import { LogoMark, Wordmark } from "./logo.tsx";
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

/** The wordmark `omfπ` (`π` in the theme's gradient, UI-221) and `@host` (UI-170). */
export function Brand({ host, className, ...props }: BrandProps) {
  return (
    <div {...props} className={cx("omfp-brand", className)}>
      <Wordmark className="omfp-brand__mark" />
      {host !== undefined && <span className="omfp-brand__host">@{host}</span>}
    </div>
  );
}

export interface WorkspaceSectionProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  name: ReactNode;
  /** The workspace folder: not shown, the name's tooltip and the head's accessible description (UI-339). */
  path?: string;
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

/** A workspace in the left column (UI-170): a `▾`/`▸` head with name (its path on hover) and count, then its rows. */
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
  const pathId = useId();
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
        {...(path === undefined ? {} : { "aria-describedby": pathId })}
        onPress={() => onExpandedChange?.(!expanded)}
        data-testid={headTestId}
      >
        <span className="omfp-wsection__glyph" aria-hidden="true">
          {expanded ? "▾" : "▸"}
        </span>
        <span className={cx("omfp-wsection__name", current && "omfp-grad-text")} title={path}>
          {name}
        </span>
        {count !== undefined && <span className="omfp-wsection__count">{count}</span>}
      </AriaButton>
      {path !== undefined && (
        <span id={pathId} hidden>
          {path}
        </span>
      )}
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

export interface HelpDialogProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  /** The title on the top border (`help` by default). */
  title?: ReactNode;
  /** The dialog's accessible name. */
  label: string;
  /** The filter field above the sections. */
  filter: ReactNode;
  /** Metadata at the right end of the top border (the closing keys). */
  end?: ReactNode;
  /** Escape, a click outside or the caller's own close key. */
  onClose?: () => void;
  /** Renders in place, without the modal overlay (the `/kit` gallery). */
  inline?: boolean;
  /** {@link HelpSection}s, or an empty-state line. */
  children?: ReactNode;
}

/**
 * The help panel (UI-198): a modal border box titled `help` on its top border (UI-187) with a filter field and the
 * sections under it, which scroll on their own; Escape or a click outside closes it, and focus returns where it was.
 */
export function HelpDialog({
  title = "help",
  label,
  filter,
  end,
  onClose,
  inline = false,
  className,
  children,
  ...props
}: HelpDialogProps) {
  const dialog = (
    <AriaDialog aria-label={label} className="omfp-help-dialog">
      <BorderBox
        {...props}
        title={title}
        level={2}
        end={end}
        className={cx("omfp-help", className)}
        bodyClassName="omfp-help__body"
      >
        <div className="omfp-help__filter">{filter}</div>
        {/* biome-ignore lint/a11y/noNoninteractiveTabindex: a scrolling region the keyboard must reach to scroll (UI-092). */}
        <section className="omfp-help__list" aria-label={label} tabIndex={0}>
          {children}
        </section>
      </BorderBox>
    </AriaDialog>
  );
  if (inline) {
    return <div className="omfp-modal omfp-help-modal">{dialog}</div>;
  }
  return (
    <ModalOverlay
      isOpen
      isDismissable
      onOpenChange={(open) => {
        if (!open) {
          onClose?.();
        }
      }}
      className="omfp-modal-overlay"
    >
      <Modal className="omfp-modal omfp-help-modal">{dialog}</Modal>
    </ModalOverlay>
  );
}

export interface HelpSectionProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  /** The label on the section's rule (`├─ normal ──┤`). */
  title: ReactNode;
  /** A line under the rule. */
  note?: ReactNode;
  /** {@link HelpRow}s. */
  children?: ReactNode;
}

/** One group of the help panel: a labelled rule (UI-187), an optional note, then its rows. */
export function HelpSection({ title, note, className, children, ...props }: HelpSectionProps) {
  return (
    <section {...props} className={cx("omfp-help__section", className)}>
      <BoxRule label={title} level={3} />
      {note !== undefined && <p className="omfp-help__note">{note}</p>}
      <dl className="omfp-help__rows">{children}</dl>
    </section>
  );
}

export interface HelpRowProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  /** Key chip labels (UI-148): `spc n`, `ctrl+c`, `:rescan`. */
  keys: readonly string[];
  /** A command's argument values, after its chips. */
  args?: string | undefined;
  description: ReactNode;
}

/** One binding or command of the help panel: its key chips, then what it does. */
export function HelpRow({ keys, args, description, className, ...props }: HelpRowProps) {
  return (
    <div {...props} className={cx("omfp-help__row", className)}>
      <dt className="omfp-help__keys">
        {keys.map((key) => (
          <Kbd key={key}>{key}</Kbd>
        ))}
        {args !== undefined && <span className="omfp-help__args">{args}</span>}
      </dt>
      <dd className="omfp-help__description">{description}</dd>
    </div>
  );
}

export interface WhichKeyProps extends Omit<HTMLAttributes<HTMLElement>, "title"> {
  /** The chip label of the keys typed so far (`spc`, `g`), after `╭─`. */
  keys: string;
  /** The panel's accessible name. */
  label: string;
  /** The key row under the groups (`esc` cancel). */
  footer?: ReactNode;
  /** {@link WhichKeyGroup}s. */
  children?: ReactNode;
}

/**
 * The which-key panel (UI-320): a popover titled `╭─ <keys>` listing, in groups laid out in columns, the keys that
 * complete a started sequence. A labelled region that never takes the focus: the keys keep going to the page.
 */
export function WhichKey({ keys, label, footer, className, children, ...props }: WhichKeyProps) {
  return (
    <section {...props} aria-label={label} className={cx("omfp-popover omfp-whichkey", className)}>
      <div className="omfp-whichkey__title" aria-hidden="true">
        ╭─ <Kbd>{keys}</Kbd>
      </div>
      <div className="omfp-whichkey__groups">{children}</div>
      {footer !== undefined && <div className="omfp-whichkey__foot">{footer}</div>}
    </section>
  );
}

export interface WhichKeyGroupProps extends Omit<HTMLAttributes<HTMLDivElement>, "title"> {
  /** The group's name above its keys (`threads`); none when the panel has a single group. */
  title?: string | undefined;
  /** {@link WhichKeyItem}s. */
  children?: ReactNode;
}

/** One group of the which-key panel: its name (read before its keys), then its keys. */
export function WhichKeyGroup({ title, className, children, ...props }: WhichKeyGroupProps) {
  return (
    <div {...props} className={cx("omfp-whichkey__group", className)}>
      {title !== undefined && <div className="omfp-whichkey__group-title">{title}</div>}
      <dl className="omfp-whichkey__rows">{children}</dl>
    </div>
  );
}

export interface WhichKeyItemProps extends Omit<HTMLAttributes<HTMLDivElement>, "children"> {
  /** The next key's chip label (`n`, `spc`). */
  chip: string;
  description: ReactNode;
  /** The binding's arguments, after the description. */
  args?: string | undefined;
  /** `prefix`: the key starts longer bindings, the description names them (`help…`); `command` by default. */
  kind?: "command" | "prefix";
  /** No command answers to the key now: dimmed. */
  disabled?: boolean;
}

/** One key of the which-key panel: its chip, an arrow, then what it runs or the group it opens. */
export function WhichKeyItem({
  chip,
  description,
  args,
  kind = "command",
  disabled = false,
  className,
  ...props
}: WhichKeyItemProps) {
  return (
    <div
      {...props}
      className={cx("omfp-whichkey__item", className)}
      data-kind={kind}
      data-disabled={disabled || undefined}
    >
      <dt className="omfp-whichkey__key">
        <Kbd>{chip}</Kbd>
        <span className="omfp-whichkey__arrow" aria-hidden="true">
          →
        </span>
      </dt>
      <dd className="omfp-whichkey__description">
        {description}
        {args !== undefined && <span className="omfp-whichkey__args">{args}</span>}
      </dd>
    </div>
  );
}

/** A thread the strip shows: its state mark, its title as the accessible name. */
export interface StripItem {
  id: string;
  title: string;
  state: ThreadState;
  selected?: boolean;
}

export interface ThreadStripProps extends Omit<HTMLAttributes<HTMLElement>, "onToggle" | "onSelect"> {
  /** Whether the left column it stands for is open. */
  open: boolean;
  onToggle: () => void;
  /** Accessible name of the toggle (show or hide the column). */
  toggleLabel: string;
  /** `id` of the column the toggle opens. */
  controls?: string;
  /** Threads needing the user and running ones. */
  threads: readonly StripItem[];
  onSelect: (id: string) => void;
  /** Class names of the toggle (forced states in the gallery). */
  toggleClassName?: string;
}

/**
 * The left column folded into a strip (UI-201, UI-208): the brand's mark `[π]`, the toggle that opens the column over
 * the content, then one state mark per thread needing the user or running, each selecting its thread.
 */
export function ThreadStrip({
  open,
  onToggle,
  toggleLabel,
  controls,
  threads,
  onSelect,
  toggleClassName,
  className,
  ...props
}: ThreadStripProps) {
  return (
    <nav {...props} className={cx("omfp-strip", className)}>
      <LogoMark className="omfp-strip__brand" aria-hidden="true" />
      <IconButton
        className={cx("omfp-strip__toggle", toggleClassName)}
        label={toggleLabel}
        icon={<Icon name="panel-left" />}
        aria-expanded={open}
        {...(controls === undefined ? {} : { "aria-controls": controls })}
        onPress={onToggle}
        data-testid="left-toggle"
      />
      <ul className="omfp-strip__threads">
        {threads.map((thread) => (
          <li key={thread.id}>
            <AriaButton
              className="omfp-strip__thread"
              aria-label={thread.title}
              data-state={thread.state}
              data-selected={thread.selected === true || undefined}
              data-testid="strip-thread"
              data-thread-id={thread.id}
              onPress={() => onSelect(thread.id)}
            >
              <StateMark state={thread.state} />
            </AriaButton>
          </li>
        ))}
      </ul>
    </nav>
  );
}

/** An area of the phone layout's bottom bar. */
export interface AreaItem {
  id: string;
  label: string;
  icon: IconName;
}

export interface AreaBarProps extends Omit<HTMLAttributes<HTMLElement>, "onSelect"> {
  /** Accessible name of the bar. */
  label: string;
  items: readonly AreaItem[];
  selected: string;
  onSelect: (id: string) => void;
}

/**
 * The phone layout's bottom bar (UI-204, UI-208): one 44 px button per area (Threads · Chat · Side), icon over label;
 * the shown area in `primary` with the gradient bar on its top edge.
 */
export function AreaBar({ label, items, selected, onSelect, className, ...props }: AreaBarProps) {
  return (
    <nav {...props} aria-label={label} className={cx("omfp-areabar", className)}>
      {items.map((item) => (
        <AriaButton
          key={item.id}
          className="omfp-areabar__item"
          aria-pressed={item.id === selected}
          data-testid="area-button"
          data-area={item.id}
          onPress={() => onSelect(item.id)}
        >
          <Icon name={item.icon} />
          <span>{item.label}</span>
        </AriaButton>
      ))}
    </nav>
  );
}
