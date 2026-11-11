/**
 * The frame on narrow and phone-sized frames (UI-200..UI-205) and the left column's splitter (UI-207): the strip the
 * left column folds into and the drawer it opens, the phone's bottom bar of areas, the `threads.toggle` command, and
 * the visual viewport's height that keeps the composer above a phone's keyboard.
 */

import { AreaBar, type AreaItem, type StripItem, ThreadStrip } from "@omfp/kit";
import { type ReactNode, type RefObject, useEffect, useLayoutEffect, useMemo, useState } from "react";
import { DRAFT_THREAD, SELECTED_THREAD, useCommand, useThreads, useWindowState } from "../sdk/index.ts";
import { strings } from "../strings/index.ts";
import type { Area } from "../view/layout.ts";
import { LEFT_DEFAULT_REM, LEFT_WIDTH, resizeLeft } from "../view/sizes.ts";
import { Resizer } from "../view/splitter.tsx";
import { threadMarkState } from "../widgets/threads/marks.ts";
import { type Breakpoint, LEFT_OPEN, MOBILE_AREA, stripThreads } from "./responsive.ts";

const text = strings.shell.responsive;

/** `id` of the left column, which the strip's toggle controls. */
export const LEFT_COLUMN_ID = "omfp-left-column";

/** Marks the frame's own controls of the drawer (strip, bottom bar): a press there is not an outside click. */
const DRAWER_CONTROL = "data-omfp-drawer";

/** The root font size in px (`rem`). */
function remPx(): number {
  return Number.parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
}

/**
 * The left column as a drawer (UI-201, UI-204): it closes when a thread or a draft is selected, on Escape, on a press
 * in the frame outside it (overlays such as menus and dialogs are not in the frame), and when the breakpoint changes.
 * Opening moves the focus to the selected thread (else the first row); closing with the focus inside returns it to the
 * strip's toggle.
 */
export function useLeftDrawer(breakpoint: Breakpoint, aside: RefObject<HTMLElement | null>): void {
  const [open, setOpen] = useWindowState(LEFT_OPEN);
  const [, setArea] = useWindowState(MOBILE_AREA);
  const [selected] = useWindowState(SELECTED_THREAD);
  const [draft] = useWindowState(DRAFT_THREAD);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new selection (or draft) closes the drawer and shows the chat.
  useEffect(() => {
    setOpen(false);
    setArea("chat");
  }, [selected, draft, setOpen, setArea]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new breakpoint closes the drawer.
  useEffect(() => setOpen(false), [breakpoint, setOpen]);
  const drawer = open && breakpoint !== "wide";
  useEffect(() => {
    const column = aside.current;
    if (!drawer || column === null) {
      return;
    }
    if (!column.contains(document.activeElement)) {
      column.querySelector<HTMLElement>(".omfp-thread[data-selected] button, .omfp-thread button")?.focus();
    }
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };
    const onPointer = (event: PointerEvent) => {
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest(".omfp-app") !== null &&
        !column.contains(target) &&
        target.closest(`[${DRAWER_CONTROL}]`) === null
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("pointerdown", onPointer, true);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("pointerdown", onPointer, true);
      if (column.contains(document.activeElement)) {
        document.querySelector<HTMLElement>('[data-testid="left-toggle"]')?.focus();
      }
    };
  }, [drawer, aside, setOpen]);
}

/** `threads.toggle` (`Space t`): opens or closes the left column on a narrow frame or a phone (UI-201). */
export function ToggleThreadsCommand({ breakpoint }: { breakpoint: Breakpoint }): ReactNode {
  const [, setOpen] = useWindowState(LEFT_OPEN);
  useCommand({
    id: "threads.toggle",
    title: strings.shell.commands.toggleThreads,
    enabled: () => breakpoint !== "wide",
    run: () => setOpen((open) => !open),
  });
  return null;
}

/** The strip of a narrow frame (UI-201): brand, toggle, and the threads needing the user or running. */
export function LeftStrip(): ReactNode {
  const [open, setOpen] = useWindowState(LEFT_OPEN);
  const [selected, select] = useWindowState(SELECTED_THREAD);
  const threads = useThreads();
  const items = useMemo(
    (): StripItem[] =>
      stripThreads([...threads.values()]).map((thread) => ({
        id: thread.id,
        title: thread.title,
        state: threadMarkState(thread.status, false),
        selected: thread.id === selected,
      })),
    [threads, selected],
  );
  return (
    <ThreadStrip
      {...{ [DRAWER_CONTROL]: "" }}
      aria-label={text.strip}
      data-testid="left-strip"
      open={open}
      onToggle={() => setOpen(!open)}
      toggleLabel={open ? text.hideThreads : text.showThreads}
      controls={LEFT_COLUMN_ID}
      threads={items}
      onSelect={(id) => select(id)}
    />
  );
}

const AREA_ITEMS: readonly AreaItem[] = [
  { id: "threads", label: text.threads, icon: "panel-left" },
  { id: "chat", label: text.chat, icon: "message-square" },
  { id: "side", label: text.side, icon: "panel-right" },
];

/** The phone's bottom bar (UI-204): Threads · Chat · Side (Side only when the view has a side area). */
export function Areas({ side }: { side: boolean }): ReactNode {
  const [open, setOpen] = useWindowState(LEFT_OPEN);
  const [area, setArea] = useWindowState(MOBILE_AREA);
  const items = side ? AREA_ITEMS : AREA_ITEMS.filter((item) => item.id !== "side");
  return (
    <AreaBar
      {...{ [DRAWER_CONTROL]: "" }}
      label={text.areas}
      data-testid="area-bar"
      items={items}
      selected={open ? "threads" : side ? area : "chat"}
      onSelect={(id) => {
        if (id === "threads") {
          setOpen(true);
        } else {
          setOpen(false);
          setArea(id as Area);
        }
      }}
    />
  );
}

/** The width of the frame (the column's parent) in `rem`; 0 before layout. */
function frameRemOf(aside: HTMLElement | null): number {
  const shell = aside?.parentElement;
  return shell === null || shell === undefined ? 0 : shell.getBoundingClientRect().width / remPx();
}

/** Sets the column's width (`undefined`: the default `--omfp-side-w` of the stylesheet). */
function showWidth(aside: HTMLElement | null, rem: number | undefined): void {
  aside?.style.setProperty("flex-basis", rem === undefined ? "" : `${rem}rem`);
}

/**
 * The splitter between the left column and the view (UI-207): drags, arrow keys (5 % of the frame) and double click
 * (back to `--omfp-side-w`) set this window's width of the column, which only it applies to the column: a drag moves
 * the column directly, without rendering the frame, and writes the width once released.
 */
export function LeftSplitter({ aside }: { aside: RefObject<HTMLElement | null> }): ReactNode {
  const [width, setWidth] = useWindowState(LEFT_WIDTH);
  const current = width ?? LEFT_DEFAULT_REM;
  const [frameRem, setFrameRem] = useState(0);
  useLayoutEffect(() => {
    const column = aside.current;
    showWidth(column, width);
    setFrameRem(frameRemOf(column));
    return () => showWidth(column, undefined);
  }, [aside, width]);
  return (
    <Resizer
      orientation="vertical"
      label={text.leftSplitter}
      value={frameRem > 0 ? (current / frameRem) * 100 : 0}
      className="omfp-frame__splitter"
      data-testid="left-splitter"
      onPreview={(delta) =>
        showWidth(aside.current, delta === undefined ? width : resizeLeft(current, delta, frameRemOf(aside.current)))
      }
      onResize={(delta) => setWidth(resizeLeft(current, delta, frameRemOf(aside.current)))}
      onReset={() => setWidth(undefined)}
    />
  );
}

/**
 * On a phone, the app is as tall as the visual viewport (`--vvh` on `<html>`, a runtime value, not a theme token), so
 * the composer stays above the on-screen keyboard (UI-205); elsewhere it fills the window.
 */
export function useVisualViewport(breakpoint: Breakpoint): void {
  useEffect(() => {
    const viewport = window.visualViewport;
    const root = document.documentElement;
    if (breakpoint !== "mobile" || viewport === null || viewport === undefined) {
      return;
    }
    const update = () => root.style.setProperty("--vvh", `${viewport.height}px`);
    update();
    viewport.addEventListener("resize", update);
    return () => {
      viewport.removeEventListener("resize", update);
      root.style.removeProperty("--vvh");
    };
  }, [breakpoint]);
}
