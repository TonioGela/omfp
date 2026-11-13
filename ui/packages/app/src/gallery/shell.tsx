/** The gallery's shell parts (UI-173): mode lines, the which-key panel, help panels, the left column and the narrow
 * and phone frame's parts. */
import {
  ActionRow,
  AreaBar,
  type AreaItem,
  Brand,
  CommandItem,
  CommandLine,
  CommandMenu,
  HelpDialog,
  HelpRow,
  HelpSection,
  HintRow,
  Icon,
  Kbd,
  Meter,
  ModeLine,
  ModeNotice,
  ModeSegment,
  Splitter,
  StateMark,
  type StripItem,
  TextField,
  ThreadRow,
  ThreadStrip,
  WhichKey,
  WhichKeyGroup,
  WhichKeyItem,
  WorkspaceSection,
} from "@omfp/kit";
import { useState } from "react";
import { Line, Section } from "./parts.tsx";

export function ModeLines() {
  return (
    <Section title="Mode line · command line · which-key" note="UI-167 · UI-168 · UI-193 · UI-320" wide>
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
      {/* Notices (UI-193): each tone, with a count, hover and focus. */}
      <ModeLine
        mode="normal"
        notice={<ModeNotice tone="attention" title="Fix flaky checkout test" body="needs you" more={2} kbd="g n" />}
      >
        <ModeSegment kind="title">omfp › Redesign kit tokens</ModeSegment>
        <ModeSegment>idle</ModeSegment>
      </ModeLine>
      <ModeLine
        mode="insert"
        notice={<ModeNotice tone="error" title="flaky" body="failed" kbd="g n" className="is-hover" />}
      >
        <ModeSegment kind="title">omfp › Redesign kit tokens</ModeSegment>
      </ModeLine>
      <ModeLine
        mode="normal"
        notice={<ModeNotice tone="ok" title="checkout" body="finished its turn" kbd="g n" className="is-focus" />}
      >
        <ModeSegment kind="title">omfp › Redesign kit tokens</ModeSegment>
      </ModeLine>
      <ModeLine mode="normal" notice={<ModeNotice tone="info" title="CI" body="is green again" kbd="g n" />}>
        <ModeSegment kind="title">omfp › Redesign kit tokens</ModeSegment>
      </ModeLine>
      <div className="omfp-gallery__narrow">
        <ModeLine mode="normal">
          <ModeSegment kind="title">omfp › A long thread title that does not fit</ModeSegment>
          <ModeSegment>opus-4.5</ModeSegment>
          <ModeSegment>
            <Meter value={48_200} max={200_000} label="Context" />
          </ModeSegment>
        </ModeLine>
        <ModeLine
          mode="normal"
          notice={<ModeNotice tone="attention" title="A long thread title that waits" body="needs you" kbd="g n" />}
        >
          <ModeSegment kind="title">omfp › A long thread title that does not fit</ModeSegment>
          <ModeSegment>opus-4.5</ModeSegment>
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
      <div className="omfp-gallery__command">
        <WhichKey
          keys="spc"
          label="Keys after spc"
          footer={
            <>
              <Kbd>esc</Kbd> <span>cancel</span>
            </>
          }
        >
          <WhichKeyGroup title="threads">
            <WhichKeyItem chip="a" description="Next thread needing attention" />
            <WhichKeyItem chip="n" description="New thread in the current workspace" />
            <WhichKeyItem chip="x" description="Archive the thread" disabled />
          </WhichKeyGroup>
          <WhichKeyGroup title="views">
            <WhichKeyItem chip="t" description="Show or hide the threads column (narrow window)" />
            <WhichKeyItem chip="v" kind="prefix" description="views…" />
          </WhichKeyGroup>
          <WhichKeyGroup title="help">
            <WhichKeyItem chip="h" kind="prefix" description="help…" />
          </WhichKeyGroup>
          <WhichKeyGroup title="other">
            <WhichKeyItem chip="o" kind="prefix" description="2 bindings…" />
            <WhichKeyItem chip="l" description="Switch view" args="logs" />
          </WhichKeyGroup>
        </WhichKey>
        <ModeLine mode="normal">
          <ModeSegment kind="title">omfp › checkout</ModeSegment>
        </ModeLine>
      </div>
    </Section>
  );
}

export function HelpPanels() {
  const filter = (value: string) => (
    <TextField
      label="Filter keys and commands"
      hideLabel
      mono
      icon={<Icon name="search" />}
      placeholder="Filter by key or description"
      defaultValue={value}
    />
  );
  const end = (
    <>
      <Kbd>?</Kbd>
      <Kbd>esc</Kbd>
      <span>close</span>
    </>
  );
  return (
    <Section title="Help" note="UI-198 · ? or :help · type to filter" wide>
      <HelpDialog inline label="Keys and commands" filter={filter("")} end={end}>
        <HelpSection title="normal">
          <HelpRow keys={["j"]} description="Next thread" />
          <HelpRow keys={["g g"]} description="First thread" />
          <HelpRow keys={["spc a", "] a", "alt+a"]} description="Next thread needing attention" />
          <HelpRow keys={["?"]} description="Show every key and command (this help)" />
        </HelpSection>
        <HelpSection title="insert · composer">
          <HelpRow keys={["esc"]} description="Leave the composer" />
          <HelpRow keys={["ctrl+c"]} description="Clear the draft, or abort when it is empty (Ctrl-C)" />
        </HelpSection>
        <HelpSection
          title="file editor · vim"
          note="While the file editor has the focus, vim's own keys apply; the app keymap leaves them alone."
        >
          <HelpRow keys={[":w", "ctrl+s"]} description="Save the file" />
        </HelpSection>
        <HelpSection title="commands">
          <HelpRow keys={[":theme"]} args="system | light | dark" description="Colour scheme (system, light, dark)" />
          <HelpRow keys={[":search", ":filter"]} description="Filter threads" />
        </HelpSection>
      </HelpDialog>
      <HelpDialog inline label="Keys and commands, no match" filter={filter("zzz")} end={end}>
        <p className="omfp-help__empty">No key or command matches</p>
      </HelpDialog>
    </Section>
  );
}

export function LeftColumn() {
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

const STRIP_THREADS: readonly StripItem[] = [
  { id: "awaiting", title: "Awaiting the user", state: "awaiting" },
  { id: "running", title: "Running thread", state: "running", selected: true },
  { id: "failed", title: "Failed thread", state: "failed" },
];

const AREAS: readonly AreaItem[] = [
  { id: "threads", label: "Threads", icon: "panel-left" },
  { id: "chat", label: "Chat", icon: "message-square" },
  { id: "side", label: "Side", icon: "panel-right" },
];

/** The narrow and phone frame's parts (UI-208): splitters, the left column's strip and the phone's bottom bar. */
export function ResponsiveParts() {
  const [open, setOpen] = useState(false);
  const [area, setArea] = useState("chat");
  return (
    <Section title="Splitter · strip · area bar" note="UI-208 · UI-201 · UI-204 · UI-206">
      <Line label="splitter">
        {(["default", "hover", "dragging"] as const).map((state) => (
          <div key={state} className="omfp-gallery__split">
            <span className="omfp-gallery__pane">{state}</span>
            <Splitter
              orientation="vertical"
              label={`Splitter, ${state}`}
              value={50}
              className={state === "hover" ? "is-hover" : undefined}
              data-dragging={state === "dragging" || undefined}
            />
            <span className="omfp-gallery__pane" />
          </div>
        ))}
      </Line>
      <Line label="strip">
        <ThreadStrip
          aria-label="Threads column"
          open={open}
          onToggle={() => setOpen(!open)}
          toggleLabel={open ? "Hide threads" : "Show threads"}
          threads={STRIP_THREADS}
          onSelect={() => undefined}
        />
        <ThreadStrip
          aria-label="Threads column, open"
          open
          onToggle={() => undefined}
          toggleLabel="Hide threads"
          toggleClassName="is-focus"
          threads={[]}
          onSelect={() => undefined}
        />
      </Line>
      <Line label="area bar">
        <AreaBar label="Areas" items={AREAS} selected={area} onSelect={setArea} className="omfp-gallery__areabar" />
      </Line>
    </Section>
  );
}
