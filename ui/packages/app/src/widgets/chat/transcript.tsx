/**
 * `omfp:transcript`: the selected thread's transcript, virtualized with `@tanstack/react-virtual` (UI-042): only the
 * items around the viewport are in the DOM, heights are measured as they render, and the view follows the bottom
 * while pinned. The transcript handles PageUp/PageDown/Home/End itself (WebKitGTK does not scroll a focused
 * container, S-11) and the transcript-scope `nav.*` commands (UI-095).
 */

import type { ThreadState } from "@omfp/core";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  type KeyboardEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { SCOPE_ATTRIBUTE, useAnnounce, useCommand, useWindowState } from "../../sdk/index.ts";
import { strings } from "../../strings/index.ts";
import { noticeRow } from "../../vm/notice.ts";
import { type Item, items, messageView, streamingView } from "../../vm/transcript.ts";
import {
  isScrollUpKey,
  keyScrollTop,
  mountScrollTop,
  type NavCommand,
  navTarget,
  type PinEvent,
  pinReducer,
} from "../../vm/virtual.ts";
import { MessageRow, NoticeRowView } from "./message.tsx";
import type { WidgetProps } from "./props.ts";
import { measurementsKey, pinKey, useSelectedThread } from "./state.ts";

/** Height assumed for an item before it is measured. */
const ESTIMATED_ITEM_PX = 120;
/** Items rendered beyond each edge of the viewport. */
const OVERSCAN = 6;

const NAV: readonly { id: NavCommand; title: string }[] = [
  { id: "nav.next", title: strings.chat.nav.next },
  { id: "nav.prev", title: strings.chat.nav.prev },
  { id: "nav.top", title: strings.chat.nav.top },
  { id: "nav.bottom", title: strings.chat.nav.bottom },
  { id: "nav.halfDown", title: strings.chat.nav.halfDown },
  { id: "nav.halfUp", title: strings.chat.nav.halfUp },
];

/** Keys typed into these edit or move within them; the transcript leaves them alone. */
const CONTROLS = 'input, textarea, select, button, [contenteditable="true"], [role="radio"], [role="checkbox"]';

export function TranscriptWidget(_props: WidgetProps) {
  const { threadId, view } = useSelectedThread();
  if (threadId === undefined || view === undefined) {
    return <ThreadPlaceholder threadId={threadId} />;
  }
  return <Transcript key={threadId} threadId={threadId} state={view.state} />;
}

/** Shown while no thread is selected or its snapshot has not arrived yet. */
export function ThreadPlaceholder({ threadId }: { threadId: string | undefined }) {
  return threadId === undefined ? (
    <div className="omfp-chat__empty" data-testid="no-thread">
      {strings.chat.noThread}
    </div>
  ) : (
    <div className="omfp-chat__empty" data-testid="thread-loading" data-thread-id={threadId}>
      {strings.chat.loading}
    </div>
  );
}

/** Announces when the agent starts and finishes working (UI-092). */
function useSettledAnnouncements(settled: boolean): void {
  const announce = useAnnounce();
  const previous = useRef(settled);
  useEffect(() => {
    if (previous.current !== settled) {
      previous.current = settled;
      announce(settled ? strings.chat.announce.settled : strings.chat.announce.working);
    }
  }, [announce, settled]);
}

/**
 * One thread's transcript; mount it with `key={threadId}` so a fresh scroll container never shows another thread's
 * position. `footer` (the pending interactions, UI-070) sits after the list inside the same scroll container.
 */
export function Transcript({ threadId, state, footer }: { threadId: string; state: ThreadState; footer?: ReactNode }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const [pin, setPin] = useWindowState(pinKey(threadId));
  const pinned = useRef(pin.pinned);
  pinned.current = pin.pinned;
  const dispatch = useCallback((event: PinEvent) => setPin((previous) => pinReducer(previous, event)), [setPin]);
  const rows = useMemo(() => items(state), [state]);
  // Mounting returns to where the user had scrolled up to, else to the bottom. Until the content is tall enough to
  // reach that position (items are measured after they render), the target is re-applied and the clamped scroll
  // events it causes are not stored; any user scrolling ends the restore.
  const restoring = useRef<number | undefined>(mountScrollTop(pin));
  const [measurements, setMeasurements] = useWindowState(measurementsKey(threadId));
  const virtualizer = useVirtualizer({
    initialMeasurementsCache: [...measurements],
    count: rows.length,
    getScrollElement: () => containerRef.current,
    estimateSize: () => ESTIMATED_ITEM_PX,
    overscan: OVERSCAN,
    getItemKey: (index) => rows[index]?.key ?? index,
    ...(restoring.current === undefined ? {} : { initialOffset: restoring.current }),
  });
  // Unmounting (a thread switch) keeps the measured sizes for the next mount.
  useEffect(() => () => setMeasurements(virtualizer.measurementsCache), [setMeasurements, virtualizer]);
  const [current, setCurrent] = useState<number | undefined>(undefined);
  useSettledAnnouncements(state.settled);

  /** Scrolls to the restore target or, while pinned, to the bottom. */
  const follow = useCallback((container: HTMLElement) => {
    const target = restoring.current;
    if (target !== undefined) {
      container.scrollTop = target;
      if (container.scrollTop >= target - 1) {
        restoring.current = undefined;
      }
    } else if (pinned.current) {
      container.scrollTop = container.scrollHeight;
    }
  }, []);
  const userScrolls = () => {
    restoring.current = undefined;
  };
  // Every render (new items, measured heights, streaming deltas) follows the bottom or the restore target.
  useLayoutEffect(() => {
    if (containerRef.current !== null) {
      follow(containerRef.current);
    }
  });
  // Growth outside this component's renders (the interactions footer) too.
  useEffect(() => {
    const container = containerRef.current;
    const content = contentRef.current;
    if (container === null || content === null || typeof ResizeObserver === "undefined") {
      return;
    }
    const observer = new ResizeObserver(() => follow(container));
    observer.observe(content);
    return () => observer.disconnect();
  }, [follow]);
  // A navigation command moved the current item: focus it once rendered.
  useEffect(() => {
    if (current !== undefined) {
      containerRef.current?.querySelector<HTMLElement>(`[data-index="${current}"]`)?.focus({ preventScroll: true });
    }
  }, [current]);

  const navigate = (command: NavCommand) => {
    const halfPage = Math.floor(virtualizer.getVirtualItems().length / 2);
    const target = navTarget(command, current, rows.length, halfPage);
    if (target < 0) {
      return;
    }
    userScrolls();
    dispatch(target === rows.length - 1 ? { type: "pin" } : { type: "userScrollUp" });
    virtualizer.scrollToIndex(target, { align: "auto" });
    setCurrent(target);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const container = event.currentTarget;
    const target = event.target;
    if (target instanceof Element && target !== container && target.closest(CONTROLS) !== null) {
      return;
    }
    const top = keyScrollTop(event.key, container.scrollTop, container.clientHeight, container.scrollHeight);
    if (isScrollUpKey(event.key) || top !== undefined || event.key === "ArrowDown") {
      userScrolls();
    }
    if (isScrollUpKey(event.key)) {
      dispatch({ type: "userScrollUp" });
    }
    if (top !== undefined) {
      event.preventDefault();
      if (event.key === "End") {
        dispatch({ type: "pin" });
      }
      container.scrollTop = top;
    }
  };

  return (
    <div
      ref={containerRef}
      className="omfp-transcript"
      data-testid="transcript"
      data-thread-id={threadId}
      data-pinned={pin.pinned}
      role="log"
      aria-label={strings.chat.transcript}
      // biome-ignore lint/a11y/noNoninteractiveTabindex: the scroll container takes focus for keyboard scrolling (UI-092)
      tabIndex={0}
      {...{ [SCOPE_ATTRIBUTE]: "transcript" }}
      onScroll={(event) => {
        // Scroll events while restoring come from clamping, not from the user.
        if (restoring.current !== undefined) {
          return;
        }
        const { scrollTop, scrollHeight, clientHeight } = event.currentTarget;
        dispatch({ type: "scroll", scrollTop, scrollHeight, clientHeight });
      }}
      onWheel={(event) => {
        userScrolls();
        if (event.deltaY < 0) {
          dispatch({ type: "userScrollUp" });
        }
      }}
      onKeyDown={onKeyDown}
      onPointerDown={() => {
        userScrolls();
        dispatch({ type: "drag", active: true });
      }}
      onPointerUp={() => dispatch({ type: "drag", active: false })}
      onPointerCancel={() => dispatch({ type: "drag", active: false })}
      onTouchStart={() => {
        userScrolls();
        dispatch({ type: "drag", active: true });
      }}
      onTouchEnd={() => dispatch({ type: "drag", active: false })}
    >
      {NAV.map(({ id, title }) => (
        <NavRegistration key={id} id={id} title={title} run={navigate} />
      ))}
      <div ref={contentRef} className="omfp-transcript__content">
        <div className="omfp-transcript__list" style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((virtual) => {
            const row = rows[virtual.index];
            return row === undefined ? null : (
              <div
                key={virtual.key}
                ref={virtualizer.measureElement}
                data-index={virtual.index}
                data-key={row.key}
                data-testid="transcript-item"
                aria-current={current === virtual.index ? true : undefined}
                tabIndex={-1}
                className="omfp-transcript__item"
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  transform: `translateY(${virtual.start}px)`,
                }}
              >
                <Row threadId={threadId} state={state} row={row} />
              </div>
            );
          })}
        </div>
        {footer}
      </div>
    </div>
  );
}

/** Registers one transcript-scope navigation command while mounted. */
function NavRegistration({ id, title, run }: { id: NavCommand; title: string; run: (command: NavCommand) => void }) {
  useCommand({ id, title, scope: "transcript", run: () => run(id) });
  return null;
}

function Row({ threadId, state, row }: { threadId: string; state: ThreadState; row: Item }) {
  const cardKey = `${threadId}/${row.key}`;
  switch (row.kind.type) {
    case "message":
      return <MessageRow cardKey={cardKey} view={messageView(state, row.kind.index)} streaming={false} />;
    case "notice": {
      const notice = state.notices[row.kind.index];
      return notice === undefined ? null : <NoticeRowView cardKey={cardKey} row={noticeRow(notice.frame)} />;
    }
    case "streaming": {
      const view = streamingView(state);
      return view === undefined ? null : <MessageRow cardKey={cardKey} view={view} streaming />;
    }
  }
}
