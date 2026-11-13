/** The gallery's marks and feedback (UI-173): state marks and badges, progress, logs, toasts and empty states. */
import {
  Badge,
  Button,
  Chip,
  Count,
  DiffStat,
  EmptyState,
  ExitCode,
  Icon,
  Kbd,
  KindMark,
  Log,
  type LogEntry,
  MessageLog,
  type MessageLogEntry,
  Meter,
  Progress,
  StateBadge,
  StateMark,
  Toast,
  ToastRegion,
  type ToastTone,
} from "@omfp/kit";
import { useLayoutEffect, useRef, useState } from "react";
import { Line, Section } from "./parts.tsx";

export function Marks() {
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

export function ProgressDemo() {
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

export function Logs() {
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

/** The window's messages as `:messages` lists them (UI-522), newest first: each tone. */
const MESSAGES: readonly MessageLogEntry[] = [
  {
    key: 1,
    tone: "ok",
    kind: "done",
    time: "14:03:12",
    dateTime: "2026-11-13T14:03:12Z",
    text: "Copied to ./widgets/board",
  },
  {
    key: 2,
    tone: "attention",
    kind: "notice",
    time: "14:02:40",
    dateTime: "2026-11-13T14:02:40Z",
    title: "Fix flaky checkout test",
    text: "needs you",
  },
  {
    key: 3,
    tone: "error",
    kind: "error",
    time: "14:01:05",
    dateTime: "2026-11-13T14:01:05Z",
    text: "No workspace widget ./widgets/bord: a long error that wraps under itself rather than running off the dialog's edge",
  },
  {
    key: 4,
    tone: "ok",
    kind: "notice",
    time: "13:59:30",
    dateTime: "2026-11-13T13:59:30Z",
    title: "checkout",
    text: "finished its turn",
  },
  {
    key: 5,
    tone: "info",
    kind: "notice",
    time: "13:58:51",
    dateTime: "2026-11-13T13:58:51Z",
    title: "CI",
    text: "is green again",
  },
];

export function Toasts() {
  const [toasts, setToasts] = useState<readonly LiveToast[]>([]);
  const next = useRef(0);
  const spawn = (tone: ToastTone, text: string) => {
    next.current += 1;
    const id = next.current;
    setToasts((all) => [...all, { id, tone, text }]);
  };
  const dismiss = (id: number) => setToasts((all) => all.filter((toast) => toast.id !== id));
  return (
    <Section title="Toasts · messages" note="UI-160 · UI-522 · buttons spawn live ones">
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
      <MessageLog entries={MESSAGES} emptyText="No messages yet" aria-label="Messages" />
      <MessageLog entries={[]} emptyText="No messages yet: notices and the results of your actions land here." />
    </Section>
  );
}

export function Empties() {
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
