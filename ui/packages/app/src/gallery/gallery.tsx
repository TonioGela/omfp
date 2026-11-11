/**
 * The `/kit` gallery (UI-173): every kit component of 09-design in every state it lists, side by side, with sizes and
 * both densities, the glyph vocabulary and a token sheet with measured contrast. Hover, pressed and focus are forced
 * with the kit's state classes (`is-hover`, `is-pressed`, `is-focus`), so the page is deterministic; the query picks
 * the scheme, theme, density, motion and root size (params.ts). The screenshot test (UI-174) waits for
 * `[data-ready=true]` and reads the contrast rows.
 */
import "./gallery.css";
import {
  ActionRow,
  ApprovalCommand,
  Badge,
  Brand,
  Button,
  Card,
  ChatHeader,
  Checkbox,
  Chip,
  Code,
  CommandItem,
  CommandLine,
  CommandMenu,
  Composer,
  ComposerBar,
  ComposerHint,
  ConfirmDialog,
  Count,
  Diff,
  DiffStat,
  EmptyState,
  ExitCode,
  HintRow,
  Icon,
  IconButton,
  InteractionActions,
  InteractionCard,
  Kbd,
  KindMark,
  ListBox,
  Log,
  type LogEntry,
  Markdown,
  Menu,
  MenuItem,
  Message,
  Meter,
  ModeLine,
  ModeSegment,
  OptionText,
  PathField,
  Progress,
  QueueChip,
  Radio,
  RadioGroup,
  ResolvedLine,
  Select,
  SelectOption,
  Separator,
  StateBadge,
  StateMark,
  StreamCaret,
  Tab,
  TabList,
  Tabs,
  TextField,
  Thinking,
  ThreadRow,
  Toast,
  ToastRegion,
  type ToastTone,
  TodoList,
  ToolCard,
  WorkspaceSection,
} from "@omfp/kit";
import { type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { applyAppearance, applyUserTheme } from "../theme/theme.ts";
import { AA, type ContrastPair, contrastRatio, parseRgb, type Rgb } from "./contrast.ts";
import forestTheme from "./matugen-forest.css" with { type: "text" };
import { type GalleryParams, galleryParams } from "./params.ts";

/** The forced interaction states shown next to the live one. */
type Forced = "live" | "hover" | "pressed" | "focus" | "disabled";

const FORCED: readonly Forced[] = ["live", "hover", "pressed", "focus", "disabled"];

/** The kit's forced state class of `state` (none for live and disabled). */
function forcedClass(state: Forced): string {
  return state === "hover" ? "is-hover" : state === "pressed" ? "is-pressed" : state === "focus" ? "is-focus" : "";
}

function Section({
  title,
  note,
  wide,
  full,
  children,
}: {
  title: string;
  note?: string;
  /** Two columns of three. */
  wide?: boolean;
  /** The whole row. */
  full?: boolean;
  children: ReactNode;
}) {
  return (
    <section
      className="omfp-gallery__section"
      data-wide={wide === true || undefined}
      data-full={full === true || undefined}
      data-testid="kit-section"
    >
      <header className="omfp-gallery__head">
        <h2 className="omfp-gallery__title omfp-grad-text omfp-grad-flow">╭─ {title}</h2>
        {note !== undefined && <span className="omfp-gallery__note">{note}</span>}
      </header>
      <div className="omfp-gallery__body">{children}</div>
    </section>
  );
}

function Label({ children }: { children: ReactNode }) {
  return <span className="omfp-gallery__label">{children}</span>;
}

/** Rows of a component, one column per forced state. */
function StateGrid({
  states = FORCED,
  rows,
}: {
  states?: readonly Forced[];
  rows: readonly { label: string; render: (state: Forced) => ReactNode }[];
}) {
  return (
    <div className="omfp-gallery__grid" style={{ gridTemplateColumns: `6rem repeat(${states.length}, max-content)` }}>
      <span />
      {states.map((state) => (
        <Label key={state}>{state}</Label>
      ))}
      {rows.map((row) => (
        <Row key={row.label} label={row.label} states={states} render={row.render} />
      ))}
    </div>
  );
}

function Row({
  label,
  states,
  render,
}: {
  label: string;
  states: readonly Forced[];
  render: (state: Forced) => ReactNode;
}) {
  return (
    <>
      <Label>{label}</Label>
      {states.map((state) => (
        <span key={state} className="omfp-gallery__cell">
          {render(state)}
        </span>
      ))}
    </>
  );
}

/** A labelled line of examples. */
function Line({ label, children }: { label?: string; children: ReactNode }) {
  return (
    <div className="omfp-gallery__line">
      {label !== undefined && <Label>{label}</Label>}
      {children}
    </div>
  );
}

function Buttons() {
  const variants = [
    { label: "primary", variant: "primary", text: "Send" },
    { label: "secondary", variant: "secondary", text: "Steer" },
    { label: "quiet", variant: "quiet", text: "Cancel" },
    { label: "danger", variant: "danger", text: "Delete", icon: <Icon name="trash-2" /> },
  ] as const;
  return (
    <Section title="Buttons" note="UI-149 · first column is live" wide>
      <StateGrid
        rows={[
          ...variants.map((v) => ({
            label: v.label,
            render: (state: Forced) => (
              <Button
                variant={v.variant}
                className={forcedClass(state)}
                isDisabled={state === "disabled"}
                icon={"icon" in v ? v.icon : undefined}
              >
                {v.text}
              </Button>
            ),
          })),
          {
            label: "icon",
            render: (state: Forced) => (
              <span className="omfp-gallery__line">
                <IconButton
                  label="Refresh"
                  icon={<Icon name="refresh-cw" />}
                  size="md"
                  className={forcedClass(state)}
                  isDisabled={state === "disabled"}
                />
                <IconButton
                  label="Settings"
                  variant="secondary"
                  size="md"
                  icon={<Icon name="settings-2" />}
                  className={forcedClass(state)}
                  isDisabled={state === "disabled"}
                />
              </span>
            ),
          },
        ]}
      />
      <div className="omfp-gallery__divider" />
      <Line label="busy">
        <Button variant="primary" isPending>
          Send
        </Button>
        <Button variant="secondary" isPending icon={<Icon name="refresh-cw" />}>
          Rescan
        </Button>
        <Button variant="danger" size="sm" isPending>
          Remove
        </Button>
      </Line>
      <div className="omfp-gallery__divider" />
      <Line label="small">
        <Button variant="primary" size="sm">
          Small
        </Button>
        <Button variant="secondary" size="sm" icon={<Icon name="plus" />}>
          thread
        </Button>
        <Button variant="quiet" size="sm" kbd="esc">
          Cancel
        </Button>
        <Button variant="danger" size="sm" icon={<Icon name="trash-2" />}>
          Delete
        </Button>
        <IconButton label="Close" icon={<Icon name="x" />} />
        <IconButton label="Settings" variant="secondary" icon={<Icon name="settings-2" />} />
      </Line>
      <Line label="default">
        <Button variant="primary">Medium</Button>
        <Button variant="secondary" icon={<Icon name="plus" />}>
          workspace
        </Button>
        <Button variant="primary" icon={<Icon name="arrow-up" />} kbd="enter">
          Send
        </Button>
        <Button variant="danger" icon={<Icon name="square" />} kbd="esc">
          Stop
        </Button>
        <IconButton label="Close" size="md" icon={<Icon name="x" />} />
        <IconButton label="Settings" size="md" variant="secondary" icon={<Icon name="settings-2" />} />
      </Line>
    </Section>
  );
}

function Fields() {
  const fieldStates: readonly Forced[] = ["live", "hover", "focus", "disabled"];
  return (
    <Section title="Text fields" note="UI-150" wide>
      <StateGrid
        states={fieldStates}
        rows={[
          {
            label: "filter",
            render: (state) => (
              <TextField
                label="Filter threads"
                hideLabel
                placeholder="Filter threads"
                icon={<Icon name="search" />}
                kbd="/"
                className={forcedClass(state)}
                isDisabled={state === "disabled"}
              />
            ),
          },
          {
            label: "plain",
            render: (state) => (
              <TextField
                label="Title"
                hideLabel
                defaultValue={state === "disabled" ? "read only" : "Redesign kit"}
                className={forcedClass(state)}
                isDisabled={state === "disabled"}
              />
            ),
          },
          {
            label: "mono",
            render: (state) => (
              <TextField
                label="Command"
                hideLabel
                mono
                defaultValue="bun test --watch"
                className={forcedClass(state)}
                isDisabled={state === "disabled"}
              />
            ),
          },
        ]}
      />
      <div className="omfp-gallery__divider" />
      <Line label="invalid">
        <TextField
          label="Folder"
          hideLabel
          mono
          defaultValue="~/projets/pmo"
          errorMessage="No such folder"
          className="omfp-gallery__field"
        />
      </Line>
      <Line label="labelled">
        <TextField
          label="Workspace name"
          description="Letters, digits and dashes."
          defaultValue="dotfiles"
          className="omfp-gallery__field"
        />
      </Line>
    </Section>
  );
}

const FOLDERS = [
  { name: "pmo", git: true, meta: "git · master" },
  { name: "pi-mono", git: true, meta: "git · main" },
  { name: "playground", git: false, meta: "folder" },
  { name: "papers", git: false, meta: "folder" },
] as const;

/** A path field as it looks while completing (UI-151): the kit's PathField markup, open, without focus. */
function StaticCompletion() {
  return (
    <div className="omfp-path-field">
      <TextField
        label="Folder"
        hideLabel
        mono
        icon={<Icon name="folder-open" />}
        kbd="tab"
        value="~/projects/p"
        className="is-focus"
      />
      <div className="omfp-popover omfp-path-field__list omfp-gallery__static" role="listbox" aria-label="Folders">
        {FOLDERS.map((folder, index) => (
          <div
            key={folder.name}
            role="option"
            aria-selected={index === 0}
            tabIndex={-1}
            className="omfp-listbox__item omfp-path-field__option"
          >
            <Icon name={folder.git ? "folder-git-2" : "folder"} />
            <span className="omfp-path-field__name">
              <span className="omfp-path-field__dir">~/projects/</span>
              <b>p</b>
              {folder.name.slice(1)}/
            </span>
            <span className="omfp-path-field__meta">{folder.meta}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

const COMPLETIONS = ["/home/you/projects/pmo", "/home/you/projects/pi-mono", "/home/you/projects/playground"];

function PathFields() {
  const [value, setValue] = useState("");
  return (
    <Section title="Path field" note="UI-151 · type ~/ · tab completes">
      <Line label="typing">
        <StaticCompletion />
      </Line>
      <Line label="live">
        <PathField
          label="Folder"
          hideLabel
          placeholder="~/projects/…"
          value={value}
          onChange={setValue}
          complete={async (path) =>
            COMPLETIONS.filter((candidate) => candidate.startsWith(path.replace(/^~/, "/home/you"))).map(
              (candidate) => ({
                name: candidate.slice(candidate.lastIndexOf("/") + 1),
                path: candidate,
                isGitRoot: candidate.endsWith("pmo"),
              }),
            )
          }
        />
      </Line>
      <Line label="no match">
        <TextField
          label="Folder"
          hideLabel
          mono
          icon={<Icon name="folder-open" />}
          kbd="tab"
          defaultValue="~/nowhere/x"
        />
      </Line>
      <Line label="invalid">
        <TextField
          label="Folder"
          hideLabel
          mono
          icon={<Icon name="folder-open" />}
          kbd="tab"
          defaultValue="~/projets/pmo"
          errorMessage="No such folder"
        />
      </Line>
      <Line label="disabled">
        <TextField
          label="Folder"
          hideLabel
          mono
          icon={<Icon name="folder-open" />}
          kbd="tab"
          defaultValue="~/projects/pmo"
          isDisabled
        />
      </Line>
    </Section>
  );
}

const MODELS = [
  { id: "opus-4.5", label: "opus-4.5", meta: "anthropic" },
  { id: "sonnet-4.5", label: "sonnet-4.5", meta: "anthropic" },
  { id: "gpt-5.1-codex", label: "gpt-5.1-codex", meta: "openai" },
];

const LEVELS = ["off", "minimal", "low", "medium", "high", "xhigh"].map((level) => ({ id: level, label: level }));

function Selects() {
  const [model, setModel] = useState<string>("opus-4.5");
  return (
    <Section title="Select" note="UI-152 · ↓ opens · j/k · enter">
      <Line>
        <Select
          label="Model"
          hideLabel
          icon={<Icon name="brain" />}
          items={MODELS}
          selectedKey={model}
          onSelectionChange={(key) => setModel(String(key))}
        />
        <Select
          label="Thinking"
          hideLabel
          prefix="thinking"
          items={LEVELS}
          selectedKey="high"
          onSelectionChange={() => undefined}
          className="is-hover"
        />
        <Select
          label="Density"
          hideLabel
          items={[
            { id: "compact", label: "compact" },
            { id: "normal", label: "normal" },
          ]}
          selectedKey="compact"
          onSelectionChange={() => undefined}
          className="is-focus"
        />
        <Select
          label="Model"
          hideLabel
          items={MODELS}
          selectedKey="sonnet-4.5"
          onSelectionChange={() => undefined}
          isDisabled
        />
      </Line>
      <div className="omfp-gallery__open">
        <Select
          label="Thinking"
          hideLabel
          prefix="thinking"
          items={LEVELS}
          selectedKey="high"
          onSelectionChange={() => undefined}
          className="is-open"
        />
        <div className="omfp-popover omfp-gallery__static omfp-gallery__options">
          <ListBox aria-label="Thinking" selectionMode="single" defaultSelectedKeys={["high"]} disabledKeys={["xhigh"]}>
            {LEVELS.map((level) => (
              <SelectOption
                key={level.id}
                id={level.id}
                textValue={level.label}
                className={level.id === "medium" ? "is-hover" : ""}
                meta={level.id === "xhigh" ? "unsupported" : undefined}
              >
                {level.id === "high" ? (
                  <>
                    <b>hi</b>gh
                  </>
                ) : (
                  level.label
                )}
              </SelectOption>
            ))}
          </ListBox>
        </div>
      </div>
    </Section>
  );
}

function Choices() {
  return (
    <Section title="Checkbox · radio" note="UI-153">
      <Line>
        <Checkbox>Unchecked</Checkbox>
        <Checkbox defaultSelected>Checked</Checkbox>
        <Checkbox isIndeterminate>Mixed</Checkbox>
      </Line>
      <Line>
        <Checkbox className="is-hover">Hover</Checkbox>
        <Checkbox defaultSelected className="is-focus">
          Focus
        </Checkbox>
        <Checkbox isDisabled>Disabled</Checkbox>
        <Checkbox isDisabled defaultSelected>
          Disabled
        </Checkbox>
      </Line>
      <RadioGroup
        label="Mode"
        hideLabel
        defaultValue="single"
        orientation="horizontal"
        className="omfp-gallery__radios"
      >
        <Radio value="single">Single</Radio>
        <Radio value="other" className="is-hover">
          Hover
        </Radio>
        <Radio value="focus" className="is-focus">
          Focus
        </Radio>
        <Radio value="disabled" isDisabled>
          Disabled
        </Radio>
      </RadioGroup>
    </Section>
  );
}

function TabsDemo() {
  return (
    <Section title="Tabs" note="UI-154 · ← → when focused">
      <Tabs defaultSelectedKey="changes">
        <TabList aria-label="Panels">
          <Tab id="workspace">Workspace</Tab>
          <Tab id="changes">Changes</Tab>
          <Tab id="todos">Todos</Tab>
          <Tab id="tree">Tree</Tab>
        </TabList>
      </Tabs>
      <Tabs defaultSelectedKey="changes" disabledKeys={["disabled"]}>
        <TabList aria-label="States">
          <Tab id="changes" className="is-focus">
            Focus
          </Tab>
          <Tab id="hover" className="is-hover">
            Hover
          </Tab>
          <Tab id="disabled">Disabled</Tab>
        </TabList>
      </Tabs>
    </Section>
  );
}

function MenuDemo() {
  return (
    <Section title="Menu" note="UI-155">
      <div className="omfp-popover omfp-gallery__static omfp-gallery__menu">
        <Menu aria-label="Workspace" disabledKeys={["archive"]}>
          <MenuItem id="rename" icon={<Icon name="pencil" />} kbd="r">
            Rename
          </MenuItem>
          <MenuItem id="copy" icon={<Icon name="copy" />} kbd="y p" className="is-hover">
            Copy path
          </MenuItem>
          <MenuItem id="rescan" icon={<Icon name="refresh-cw" />} kbd=":rescan" className="is-focus">
            Rescan
          </MenuItem>
          <MenuItem id="archive" icon={<Icon name="archive" />}>
            Archive all threads
          </MenuItem>
          <Separator />
          <MenuItem id="remove" icon={<Icon name="trash-2" />} kbd="d d" danger>
            Remove workspace
          </MenuItem>
        </Menu>
      </div>
    </Section>
  );
}

function Dialogs() {
  const [open, setOpen] = useState(false);
  const message = (
    <>
      Its folder <code>~/.local/share/omfp/ws/dotfiles</code> is deleted. Linked projects are kept.
    </>
  );
  const common = {
    isOpen: true,
    onOpenChange: () => undefined,
    title: "Remove workspace “dotfiles”?",
    message,
    confirmLabel: "Remove",
    tone: "danger",
    cancelKbd: "esc",
    confirmKbd: "enter",
    confirmIcon: <Icon name="trash-2" />,
    onConfirm: () => undefined,
    inline: true,
  } as const;
  return (
    <Section title="Dialog" note="UI-156 · focus trapped · esc closes" full>
      <div className="omfp-gallery__stages">
        <div className="omfp-gallery__stage">
          <ConfirmDialog {...common} testId="dialog-open">
            <Checkbox>Also delete its 2 threads</Checkbox>
          </ConfirmDialog>
        </div>
        <div className="omfp-gallery__stage">
          <ConfirmDialog {...common} testId="dialog-busy" busy />
        </div>
        <div className="omfp-gallery__stage">
          <ConfirmDialog
            {...common}
            testId="dialog-error"
            busy={false}
            error="The folder is in use by a running thread."
          />
        </div>
      </div>
      <Line>
        <Button variant="secondary" size="sm" onPress={() => setOpen(true)}>
          Open as modal
        </Button>
      </Line>
      <ConfirmDialog
        isOpen={open}
        onOpenChange={setOpen}
        title="Remove workspace “dotfiles”?"
        message={message}
        confirmLabel="Remove"
        tone="danger"
        cancelKbd="esc"
        confirmKbd="enter"
        onConfirm={() => undefined}
      />
    </Section>
  );
}

function Marks() {
  const [removed, setRemoved] = useState(false);
  return (
    <Section title="Badges · chips · keys" note="UI-157 · UI-148">
      <Line>
        <StateBadge state="running" />
        <StateBadge state="awaiting">awaiting input</StateBadge>
        <StateBadge state="failed" />
        <StateBadge state="idle" />
        <StateBadge state="unread" />
        <StateBadge state="running" fresh>
          fresh
        </StateBadge>
      </Line>
      <Line>
        <Badge>default</Badge>
        <Badge tone="primary">primary</Badge>
        <Badge tone="success">success</Badge>
        <Badge tone="warning">warning</Badge>
        <Badge tone="error">error</Badge>
      </Line>
      <Line>
        <Chip icon={<Icon name="link-2" />}>pmo</Chip>
        <Chip icon={<Icon name="git-fork" />}>omp</Chip>
        <Chip icon={<Icon name="copy" />}>matugen</Chip>
        {removed ? (
          <Chip variant="ghost" icon={<Icon name="plus" />}>
            model
          </Chip>
        ) : (
          <Chip onRemove={() => setRemoved(true)} removeLabel="Remove opus-4.5">
            opus-4.5
          </Chip>
        )}
        <Chip variant="ghost">+ repo</Chip>
        <Count>12</Count>
      </Line>
      <Line>
        <KindMark kind="worktree" />
        <KindMark kind="clone" />
        <KindMark kind="link" />
        <ExitCode code={0} />
        <ExitCode code={1} />
        <DiffStat added={128} removed={14} />
      </Line>
      <Line label="marks">
        {(["running", "awaiting", "failed", "unread", "idle", "done"] as const).map((state) => (
          <span key={state} className="omfp-gallery__mark">
            <StateMark state={state} />
            {state}
          </span>
        ))}
      </Line>
      <Line label="keys">
        {["i", "esc", "g g", "ctrl+d", "spc n", "alt+enter", "shift+tab", ":rescan"].map((key) => (
          <Kbd key={key}>{key}</Kbd>
        ))}
      </Line>
      <p className="omfp-gallery__text">
        Press <Kbd>spc n</Kbd> for a new thread, <Kbd>i</Kbd> to write and <Kbd>esc</Kbd> to leave insert mode.
      </p>
    </Section>
  );
}

function ProgressDemo() {
  return (
    <Section title="Progress · meter" note="UI-158">
      <Progress label="Nothing yet" value={0} />
      <Progress label="Indexing session files" value={62} />
      <Progress label="Done" value={100} />
      <Progress label="Cloning matugen" />
      <Line label="meter">
        <Meter value={48_200} max={200_000} label="Context" />
        <Meter value={171_000} max={200_000} label="Context" />
      </Line>
    </Section>
  );
}

const LOG: readonly LogEntry[] = [
  { time: "14:02:11", level: "info", text: "bun test v1.4.4" },
  { time: "14:02:12", level: "info", text: "packages/kit 142 pass" },
  { time: "14:02:12", level: "warn", text: "slow test Tabs › keyboard (412ms)" },
  { time: "14:02:13", level: "err", text: "transport › reconnect after close" },
  { time: "14:02:13", level: "info", text: "318 pass · 1 fail · 2.1s" },
];

/** A log the reader scrolled up in: it stays put while lines arrive. */
function ScrolledLog() {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const log = ref.current?.querySelector(".omfp-log");
    if (log instanceof HTMLElement) {
      log.scrollTop = 0;
      log.dispatchEvent(new Event("scroll"));
    }
  }, []);
  return (
    <div ref={ref}>
      <Log lines={[...LOG, ...LOG]} className="omfp-gallery__short-log" aria-label="Scrolled log" />
    </div>
  );
}

function Logs() {
  return (
    <Section title="Log" note="UI-159 · job/bun-test">
      <Log lines={LOG} aria-label="Streaming log" />
      <Line label="empty">
        <Log lines={[]} aria-label="Empty log" className="omfp-gallery__grow" />
      </Line>
      <Line label="scrolled up">
        <span className="omfp-gallery__grow">
          <ScrolledLog />
        </span>
      </Line>
    </Section>
  );
}

interface LiveToast {
  readonly id: number;
  readonly tone: ToastTone;
  readonly text: string;
}

function Toasts() {
  const [toasts, setToasts] = useState<readonly LiveToast[]>([]);
  const next = useRef(0);
  const spawn = (tone: ToastTone, text: string) => {
    next.current += 1;
    const id = next.current;
    setToasts((all) => [...all, { id, tone, text }]);
  };
  const dismiss = (id: number) => setToasts((all) => all.filter((toast) => toast.id !== id));
  return (
    <Section title="Toasts" note="UI-160 · buttons spawn live ones">
      <Toast onDismiss={() => undefined}>Theme changed by another window</Toast>
      <Toast tone="success">
        Workspace <b>omfp</b> rescanned
      </Toast>
      <Toast
        tone="error"
        action={
          <Button size="sm" variant="secondary">
            Retry
          </Button>
        }
      >
        Daemon unreachable · retrying in 4s
      </Toast>
      <Line>
        <Button size="sm" variant="quiet" onPress={() => spawn("info", "Theme changed by another window")}>
          info
        </Button>
        <Button size="sm" variant="quiet" onPress={() => spawn("success", "Workspace omfp rescanned")}>
          success
        </Button>
        <Button size="sm" variant="quiet" onPress={() => spawn("error", "Daemon unreachable")}>
          error
        </Button>
      </Line>
      <ToastRegion>
        {toasts.map((toast) => (
          <Toast key={toast.id} tone={toast.tone} onDismiss={() => dismiss(toast.id)}>
            {toast.text}
          </Toast>
        ))}
      </ToastRegion>
    </Section>
  );
}

function Empties() {
  return (
    <Section title="Empty states" note="UI-161">
      <div className="omfp-gallery__pair">
        <EmptyState icon="inbox" title="No threads yet">
          Start one with <Kbd>spc n</Kbd> or the + thread row.
        </EmptyState>
        <EmptyState
          icon="git-branch"
          title="Working tree clean"
          action={
            <Button size="sm" variant="quiet" icon={<Icon name="refresh-cw" />}>
              Rescan
            </Button>
          }
        >
          No changes in pmo, omp, matugen.
        </EmptyState>
      </div>
      <EmptyState icon="search" title="No result" bare className="omfp-gallery__bare">
        Nothing matches “tokns”.
      </EmptyState>
    </Section>
  );
}

function Cards() {
  const [expanded, setExpanded] = useState(true);
  return (
    <Section title="Cards" note="UI-162">
      <Card title="Card" actions={<Count>3</Count>}>
        A transparent card with a rule border; a head and a body under a rule.
      </Card>
      <Card title="Collapsible" collapsible={{ expanded, onExpandedChange: setExpanded }}>
        Its head toggles the body.
      </Card>
      <Card title="Collapsed" collapsible={{ expanded: false, onExpandedChange: () => undefined }} />
      <Card title="Failed" tone="error">
        A toned card: the border takes the status color.
      </Card>
    </Section>
  );
}

const PATCH = `@@ -1,5 +1,6 @@ export function Button
 import { Kbd } from "./kbd.tsx";
-export type ButtonSize = "sm";
+export type ButtonSize = "sm" | "md";
+export type ButtonVariant = "primary" | "secondary";
 const DEFAULT = "secondary";
\u0020
 export function Button() {}
`;

function ToolCards() {
  const [open, setOpen] = useState(true);
  return (
    <Section title="Tool cards" note="UI-163">
      <ToolCard status="running" name="bash" arg="bun test packages/kit" meta="4.2s" expanded={false}>
        <span />
      </ToolCard>
      <ToolCard status="done" name="read" arg="ui/packages/kit/src/kit.css" meta="1,148 lines" expanded={false}>
        <span />
      </ToolCard>
      <ToolCard
        status="done"
        name="bash"
        arg="ls -la ui/packages"
        meta={
          <>
            0.1s <ExitCode code={0} />
          </>
        }
        expanded={open}
        onExpandedChange={setOpen}
      >
        <Code block code={"app\nclient\ncore\nkit\nprotocol"} />
      </ToolCard>
      <ToolCard
        status="failed"
        name="bash"
        arg="cargo xtask spec-trace"
        meta={
          <>
            2.3s <ExitCode code={1} />
          </>
        }
        expanded
      >
        <Code block code="error: UI-174 is not cited by any test" />
      </ToolCard>
      <ToolCard status="done" name="todo" arg="3 items" expanded>
        <TodoList
          items={[
            { text: "Tokens in one namespace", status: "done" },
            { text: "Restyle the kit", status: "active" },
            { text: "Screenshot test", status: "pending" },
          ]}
        />
      </ToolCard>
      <ToolCard
        status="done"
        name="edit"
        arg="ui/packages/kit/src/button.tsx"
        meta={<DiffStat added={2} removed={1} />}
        expanded
      >
        <Diff patch={PATCH} language="ts" />
      </ToolCard>
      <ToolCard status="done" name="glob" arg="**/*.css" meta="no body" />
    </Section>
  );
}

function Interactions() {
  return (
    <Section title="Approval · ask" note="UI-164">
      <InteractionCard title="Run a command?" meta="1/2" countdown="0:42">
        <ApprovalCommand command="rm -rf ui/packages/app/dist" />
        <InteractionActions>
          <Button variant="primary" size="sm" kbd="enter">
            Approve
          </Button>
          <Button variant="secondary" size="sm">
            Always
          </Button>
          <Button variant="danger" size="sm" kbd="esc">
            Deny
          </Button>
        </InteractionActions>
      </InteractionCard>
      <InteractionCard title="Run a command?" countdown="0:04" low>
        <ApprovalCommand command="git push --force" />
        <InteractionActions>
          <Button variant="primary" size="sm" isPending>
            Approve
          </Button>
          <Button variant="danger" size="sm" isDisabled>
            Deny
          </Button>
        </InteractionActions>
      </InteractionCard>
      <InteractionCard title="Which palette should the built-in theme use?" meta="2/3">
        <RadioGroup label="Palette" hideLabel defaultValue="synth">
          <Radio value="synth">
            <OptionText label="Synth" description="cyan into indigo, the default" />
          </Radio>
          <Radio value="lagoon">
            <OptionText label="Lagoon" description="teal-green into deep blue" />
          </Radio>
        </RadioGroup>
        <Checkbox defaultSelected>
          <OptionText label="Bundle the fonts" description="IBM Plex Serif and Commit Mono" />
        </Checkbox>
        <TextField label="Something else" hideLabel placeholder="Something else…" />
        <InteractionActions>
          <Button variant="primary" size="sm" kbd="enter">
            Answer
          </Button>
        </InteractionActions>
      </InteractionCard>
      <ResolvedLine outcome="approved" question="Run a command? rm -rf ui/packages/app/dist" answer="Approved" />
      <ResolvedLine outcome="denied" question="Run a command? git push --force" answer="Denied" />
      <ResolvedLine outcome="answered" question="Which palette should the built-in theme use?" answer="Synth" />
    </Section>
  );
}

function Diffs() {
  return (
    <Section title="Diff" note="UI-165">
      <Diff patch={PATCH} language="ts" />
      <Diff patch={"@@ -0,0 +1,2 @@\n+added line one\n+added line two\n"} />
      <Diff patch={"@@ -1,2 +0,0 @@\n-removed line one\n-removed line two\n"} />
      <Line label="empty">
        <Diff patch="" />
      </Line>
      <Line label="binary">
        <Diff patch="" binary />
      </Line>
      <Line label="stats">
        <DiffStat added={2} removed={1} />
        <DiffStat added={1204} removed={0} />
      </Line>
    </Section>
  );
}

const MARKDOWN = `## Release plan

Here is the **plan**, starting with \`cargo xtask spec-trace\`: build the web bundle, then run the journeys and look at
every screenshot before calling it done.

- tokens in one namespace
- the kit restyled, then the app

\`\`\`rust
fn main() {
    println!("hello from the release plan");
}
\`\`\`
`;

function CodeDemo() {
  return (
    <Section title="Code · prose" note="UI-166 · UI-144">
      <Code block language="ts" code={'// the built-in palette\nconst theme = "synth";\nexport default theme;'} />
      <p className="omfp-prose omfp-gallery__prose">
        Inline code like <Code code="--omfp-grad" /> sits in prose at 0.82em.
      </p>
      <Markdown text={MARKDOWN} />
    </Section>
  );
}

function ModeLines() {
  return (
    <Section title="Mode line · command line" note="UI-167 · UI-168" wide>
      <ModeLine mode="normal">
        <ModeSegment kind="title">omfp › Redesign kit tokens</ModeSegment>
        <ModeSegment>
          <StateMark state="running" /> running
        </ModeSegment>
        <ModeSegment>opus-4.5</ModeSegment>
        <ModeSegment>
          <Meter value={48_200} max={200_000} label="Context" />
        </ModeSegment>
        <ModeSegment>
          <Icon name="git-branch" />
          master
        </ModeSegment>
      </ModeLine>
      <ModeLine mode="insert">
        <ModeSegment kind="title">omfp › Redesign kit tokens</ModeSegment>
        <ModeSegment kind="message">Copied the path</ModeSegment>
        <ModeSegment>
          <Meter value={171_000} max={200_000} label="Context" />
        </ModeSegment>
      </ModeLine>
      <ModeLine mode="command">
        <CommandLine>
          <span>rescan</span>
        </CommandLine>
      </ModeLine>
      <ModeLine mode="normal">
        <ModeSegment kind="title">omfp › Redesign kit tokens</ModeSegment>
        <ModeSegment kind="error">Unknown command: rescna</ModeSegment>
      </ModeLine>
      <div className="omfp-gallery__narrow">
        <ModeLine mode="normal">
          <ModeSegment kind="title">omfp › A long thread title that does not fit</ModeSegment>
          <ModeSegment>opus-4.5</ModeSegment>
          <ModeSegment>
            <Meter value={48_200} max={200_000} label="Context" />
          </ModeSegment>
        </ModeLine>
      </div>
      <div className="omfp-gallery__pair">
        <div className="omfp-gallery__command">
          <CommandMenu
            footer={
              <>
                <Kbd>tab</Kbd> <span>complete</span>
                <Kbd>enter</Kbd> <span>run</span>
                <Kbd>esc</Kbd> <span>close</span>
              </>
            }
          >
            <CommandItem command="rescan" matched={2} description="Rescan the workspace folder" selected />
            <CommandItem command="rename" matched={2} description="Rename the thread" />
            <CommandItem command="remove-workspace" matched={2} description="Remove the workspace" />
          </CommandMenu>
          <ModeLine mode="command">
            <CommandLine>
              <input className="omfp-cmd__input" aria-label="Command" defaultValue="re" readOnly />
            </CommandLine>
          </ModeLine>
        </div>
        <div className="omfp-gallery__command">
          <ModeLine mode="command">
            <CommandLine>
              <input className="omfp-cmd__input" aria-label="Empty command" defaultValue="" readOnly />
            </CommandLine>
          </ModeLine>
          <ModeLine mode="command">
            <CommandLine>
              <input className="omfp-cmd__input" aria-label="No completion" defaultValue="xyz" readOnly />
            </CommandLine>
          </ModeLine>
        </div>
      </div>
    </Section>
  );
}

function LeftColumn() {
  return (
    <Section title="Left column" note="UI-169 · UI-170">
      <div className="omfp-gallery__side">
        <Brand host="devbox" />
        <WorkspaceSection name="omfp" path="~/.local/share/omfp/ws/omfp" count={9} expanded current>
          <ThreadRow state="idle" title="Idle thread" meta="3d" />
          <ThreadRow state="running" title="Running thread" meta="now" />
          <ThreadRow state="awaiting" title="Awaiting the user" meta="2m" />
          <ThreadRow state="failed" title="Failed thread" meta="1h" />
          <ThreadRow state="unread" title="Unread reply" meta="5m" />
          <ThreadRow state="idle" title="Selected thread" meta="1d" selected />
          <ThreadRow state="idle" title="Hovered thread" meta="2d" className="is-hover" onDelete={() => undefined} />
          <ThreadRow state="idle" title="Focused thread" meta="2d" className="is-focus" onDelete={() => undefined} />
          <ThreadRow state="awaiting" title="Fresh: just started awaiting" meta="now" fresh />
          <ActionRow icon="plus">thread</ActionRow>
          <ActionRow kind="archived" icon="archive">
            Archived (4)
          </ActionRow>
        </WorkspaceSection>
        <WorkspaceSection name="dotfiles" path="~/dotfiles" count={2} expanded={false} headClassName="is-hover" />
        <WorkspaceSection name="notes" path="~/notes" count={0} expanded headClassName="is-focus">
          <HintRow>No threads · spc n starts one</HintRow>
        </WorkspaceSection>
      </div>
    </Section>
  );
}

function Transcript() {
  const [open, setOpen] = useState(true);
  return (
    <Section title="Transcript" note="UI-171 · UI-147" wide>
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
          The tokens live in two places; the kit's ad-hoc ones go away without aliases.
        </Thinking>
        <Message author="assistant" name="omp">
          <Markdown text={"Done: the tokens now live in **one namespace**, `--omfp-*`."} />
        </Message>
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

function Composers() {
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
              <Kbd>alt+enter</Kbd> steer
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

function DensityStrip() {
  return (
    <Section title="Density" note="UI-127 · UI-128 · compact and normal" wide>
      {(["compact", "normal"] as const).map((density) => (
        <div key={density} className="omfp-gallery__density" data-density={density}>
          <Label>{density}</Label>
          <Button variant="primary">Send</Button>
          <Button size="sm">Small</Button>
          <TextField label="Filter" hideLabel placeholder="Filter" icon={<Icon name="search" />} />
          <Select label="Level" hideLabel items={LEVELS} selectedKey="high" onSelectionChange={() => undefined} />
          <span className="omfp-gallery__row">
            <ThreadRow state="running" title="A thread row" meta="now" />
          </span>
        </div>
      ))}
    </Section>
  );
}

const GLYPHS: readonly (readonly [glyph: string, meaning: string, className: string])[] = [
  ["◆", "identity, attention, awaiting", "omfp-gallery__glyph--warn"],
  ["▸ ▾", "current choice, disclosure", "omfp-gallery__glyph--primary"],
  ["●", "finished tool call, unread", "omfp-gallery__glyph--ok"],
  ["○", "idle thread", "omfp-gallery__glyph--outline"],
  ["⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏", "running (spinner frames)", "omfp-gallery__glyph--primary"],
  ["╭─", "titled group", "omfp-gallery__glyph--primary"],
  ["·", "mode-line separator", "omfp-gallery__glyph--rule"],
  ["@", "daemon host", "omfp-gallery__glyph--muted"],
];

function Glyphs() {
  return (
    <Section title="Glyphs" note="UI-122">
      <dl className="omfp-gallery__glyphs">
        {GLYPHS.map(([glyph, meaning, className]) => (
          <div key={glyph} className="omfp-gallery__glyph-row">
            <dt className={className}>{glyph}</dt>
            <dd>{meaning}</dd>
          </div>
        ))}
      </dl>
    </Section>
  );
}

const TYPE_SCALE = ["2xs", "xs", "sm", "md", "lg", "xl", "code"] as const;
const SPACES = ["s1", "s2", "s3", "s4", "s5", "s6", "s8"] as const;
const RADII = ["r-chip", "r-ctl", "r-row", "r-card"] as const;

const ROLES = [
  "primary",
  "on-primary",
  "primary-container",
  "on-primary-container",
  "secondary",
  "on-secondary",
  "secondary-container",
  "on-secondary-container",
  "tertiary",
  "on-tertiary",
  "tertiary-container",
  "on-tertiary-container",
  "error",
  "on-error",
  "error-container",
  "on-error-container",
  "surface",
  "on-surface",
  "surface-variant",
  "on-surface-variant",
  "surface-container-lowest",
  "surface-container-low",
  "surface-container",
  "surface-container-high",
  "surface-container-highest",
  "surface-dim",
  "surface-bright",
  "inverse-surface",
  "inverse-on-surface",
  "inverse-primary",
  "outline",
  "outline-variant",
  "shadow",
  "scrim",
  "success",
  "on-success",
  "warning",
  "on-warning",
] as const;

const DERIVED = [
  "grad",
  "grad-loop",
  "grad-soft",
  "grad-end",
  "ok",
  "warn",
  "rule",
  "rule-strong",
  "hover",
  "press",
  "pop-bg",
  "pop-border",
  "code-bg",
  "side-bg",
  "kbd-bg",
] as const;

/**
 * Text roles and gradient stops on the backgrounds they sit on (UI-174, UI-135): each must reach AA. Names are tokens
 * without `--omfp-`. The derived status colors (`ok`, `warn`, UI-136) are measured too but listed apart
 * ({@link STATUS_PAIRS}): their AA is not part of the check.
 */
const TEXT_PAIRS: readonly (readonly [text: string, background: string])[] = [
  ["on-surface", "surface"],
  ["on-surface", "surface-container-lowest"],
  ["on-surface", "surface-container-low"],
  ["on-surface", "surface-container"],
  ["on-surface", "surface-container-high"],
  ["on-surface", "surface-container-highest"],
  ["on-surface-variant", "surface"],
  ["on-surface-variant", "surface-container-lowest"],
  ["on-surface-variant", "surface-container-low"],
  ["on-surface-variant", "surface-container"],
  ["primary", "surface"],
  ["primary", "surface-container-lowest"],
  ["primary", "surface-container-low"],
  ["tertiary", "surface"],
  ["tertiary", "surface-container"],
  ["error", "surface"],
  ["error", "surface-container-low"],
  ["on-primary", "primary"],
  ["on-primary", "tertiary"],
  ["on-tertiary", "tertiary"],
  ["on-error", "error"],
  ["on-secondary-container", "secondary-container"],
  ["inverse-on-surface", "inverse-surface"],
  // Gradient text (UI-135): every stop of --omfp-grad on the app and side backgrounds.
  ["grad-end", "surface"],
  ["grad-end", "surface-container-lowest"],
  ["tertiary", "surface-container-lowest"],
];

/** Resolves color tokens to sRGB through the page (`getComputedStyle` → canvas), for any notation. */
function colorReader(): ((token: string) => Rgb | undefined) | undefined {
  const canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (context === null) {
    return undefined;
  }
  const probe = document.createElement("span");
  document.body.append(probe);
  return (token) => {
    probe.style.color = `var(--omfp-${token})`;
    const computed = getComputedStyle(probe).color;
    const direct = parseRgb(computed);
    if (direct !== undefined && !computed.startsWith("rgba")) {
      return direct;
    }
    context.clearRect(0, 0, 1, 1);
    context.fillStyle = computed;
    context.fillRect(0, 0, 1, 1);
    const [red = 0, green = 0, blue = 0] = context.getImageData(0, 0, 1, 1).data;
    return [red, green, blue];
  };
}

/** Status colors as text (UI-136, UI-137): shown with their ratio, not checked. */
const STATUS_PAIRS: readonly (readonly [text: string, background: string])[] = [
  ["ok", "surface"],
  ["warn", "surface"],
  ["ok", "surface-container-lowest"],
  ["warn", "surface-container-lowest"],
];

interface MeasuredPair extends ContrastPair {
  /** Part of the AA check (UI-174). */
  readonly checked: boolean;
}

function measure(): MeasuredPair[] {
  const read = colorReader();
  if (read === undefined) {
    return [];
  }
  return [
    ...TEXT_PAIRS.map((pair) => [pair, true] as const),
    ...STATUS_PAIRS.map((pair) => [pair, false] as const),
  ].flatMap(([[text, background], checked]) => {
    const fore = read(text);
    const back = read(background);
    return fore === undefined || back === undefined
      ? []
      : [{ text, background, ratio: contrastRatio(fore, back), checked }];
  });
}

function TokenSheet({ onMeasured }: { onMeasured: () => void }) {
  const [pairs, setPairs] = useState<readonly MeasuredPair[] | undefined>();
  const measured = useRef(onMeasured);
  measured.current = onMeasured;
  useLayoutEffect(() => {
    setPairs(measure());
  }, []);
  useEffect(() => {
    if (pairs !== undefined) {
      measured.current();
    }
  }, [pairs]);
  return (
    <>
      <Section title="Type scale" note="UI-126 · rem × [ui] font_size">
        {TYPE_SCALE.map((size) => (
          <div key={size} className="omfp-gallery__type" style={{ fontSize: `var(--omfp-fs-${size})` }}>
            <Label>fs-{size}</Label> Crush chrome 0123
          </div>
        ))}
        <div className="omfp-gallery__type">
          <Label>fs-prose</Label>
          <span className="omfp-prose">
            Prose reads in IBM Plex Serif, <em>italic</em> and <strong>bold</strong>.
          </span>
        </div>
        <div className="omfp-gallery__type omfp-gallery__weights">
          <span style={{ fontWeight: "var(--omfp-fw-regular)" }}>400 regular</span>
          <span style={{ fontWeight: "var(--omfp-fw-medium)" }}>600 medium</span>
          <span style={{ fontWeight: "var(--omfp-fw-strong)" }}>700 strong</span>
        </div>
      </Section>
      <Section title="Spacing · radii" note="UI-127 · UI-129">
        {SPACES.map((space) => (
          <div key={space} className="omfp-gallery__space">
            <Label>{space}</Label>
            <span className="omfp-gallery__bar" style={{ inlineSize: `var(--omfp-${space})` }} />
          </div>
        ))}
        <Line>
          {RADII.map((radius) => (
            <span key={radius} className="omfp-gallery__radius" style={{ borderRadius: `var(--omfp-${radius})` }}>
              {radius}
            </span>
          ))}
          <span className="omfp-gallery__radius" style={{ borderRadius: "999px" }}>
            pill
          </span>
        </Line>
      </Section>
      <Section title="Colors" note="UI-134 · UI-135 · UI-136 · UI-139" wide>
        <div className="omfp-gallery__swatches">
          {[...ROLES, ...DERIVED].map((token) => (
            <div key={token} className="omfp-gallery__swatch">
              <span className="omfp-gallery__chip" style={{ background: `var(--omfp-${token})` }} />
              <span>{token}</span>
            </div>
          ))}
        </div>
      </Section>
      <Section title="Contrast" note={`UI-174 · AA ${AA}:1`}>
        <table className="omfp-gallery__contrast">
          <tbody>
            {(pairs ?? []).map((pair) => (
              <tr
                key={`${pair.text}/${pair.background}`}
                data-testid="contrast"
                data-text={pair.text}
                data-background={pair.background}
                data-ratio={pair.ratio.toFixed(2)}
                data-pass={pair.ratio >= AA}
                data-checked={pair.checked}
              >
                <td>
                  <span
                    className="omfp-gallery__sample"
                    style={{ color: `var(--omfp-${pair.text})`, background: `var(--omfp-${pair.background})` }}
                  >
                    Aa
                  </span>
                </td>
                <td>
                  {pair.text} on {pair.background}
                  {pair.checked ? "" : " (status)"}
                </td>
                <td className="omfp-gallery__ratio">{pair.ratio.toFixed(2)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </Section>
    </>
  );
}

/**
 * Puts the query's appearance and theme on the document (the user theme is replaced by the fixture or nothing), before
 * the first render, so what the page measures is in its final colors.
 */
function applyGalleryParams(params: GalleryParams): void {
  applyAppearance(document.documentElement, {
    scheme: params.scheme,
    density: params.density,
    fontSize: params.fontSize,
    motion: params.motion,
    fonts: null,
  });
  applyUserTheme(document, params.theme === "matugen-forest" ? (forestTheme as string) : null);
  document.title = "omfp kit";
}

/** Two frames: what the last state change shows has been laid out and painted. */
function afterPaint(): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  return promise;
}

/** The `/kit` page. */
export function Gallery({ search }: { search: string }) {
  const [params] = useState(() => {
    const parsed = galleryParams(search);
    applyGalleryParams(parsed);
    return parsed;
  });
  const [measured, setMeasured] = useState(false);
  const [ready, setReady] = useState(false);
  useEffect(() => {
    if (!measured) {
      return;
    }
    let live = true;
    void (async () => {
      await document.fonts.ready;
      await afterPaint();
      await document.fonts.ready;
      if (live) {
        setReady(true);
      }
    })();
    return () => {
      live = false;
    };
  }, [measured]);
  return (
    <main className="omfp-gallery" data-testid="kit-gallery" data-ready={ready}>
      <header className="omfp-gallery__top">
        <h1 className="omfp-gallery__page-title omfp-grad-text omfp-grad-flow">◆ omfp kit</h1>
        <span className="omfp-gallery__note">
          Crush · {params.theme} · {params.scheme} · {params.density} · motion {params.motion} · {params.fontSize}px
        </span>
      </header>
      <div className="omfp-gallery__sections">
        <Buttons />
        <Glyphs />
        <Fields />
        <PathFields />
        <Selects />
        <Choices />
        <TabsDemo />
        <MenuDemo />
        <Marks />
        <Dialogs />
        <ProgressDemo />
        <Logs />
        <Toasts />
        <ModeLines />
        <Empties />
        <Cards />
        <ToolCards />
        <Interactions />
        <Diffs />
        <CodeDemo />
        <LeftColumn />
        <Transcript />
        <Composers />
        <DensityStrip />
        <TokenSheet onMeasured={() => setMeasured(true)} />
      </div>
    </main>
  );
}
