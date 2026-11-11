/**
 * `omfp/sdk` (AGT-016): what every widget builds on, built-in or written by an agent (docs/specs/08-agent-control.md);
 * the bundle exposes this module to widget modules through the page's import map (AGT-012), and
 * `crates/omfp-daemon/src/widgets/docs.md` documents it for agents.
 *
 * A window creates one {@link OmfpHost} over its protocol client ({@link createHost}) and provides it with
 * {@link OmfpProvider}; widgets then use only the hooks:
 *
 * | Hook | Gives |
 * | --- | --- |
 * | `useConnection()` | connection status (connecting, connected, reconnecting, …) |
 * | `useIndex()`, `useThreads()`, `useThreadSummary(id)`, `useWorkspaces()` | the `index` topic |
 * | `useThread(id)` | the thread's `ThreadView` (reducer state + closed interactions), following `thread/<id>` |
 * | `useWorkspace(id)` | the workspace and its repositories' git status, following `workspace/<id>` |
 * | `useViews(id)` | the workspace's views manifest, widget folders and stored values, following `views/<id>` |
 * | `useWidget()` | the widget instance being rendered: `{ instance, reference, workspaceId }` |
 * | `useStored(key, initial)` | `[value, set, loaded]`: JSON kept by the daemon per workspace, instance and key (AGT-018) |
 * | `useJob(command)` | `{ lines, running, exitCode, signal, restart, kill }`: one daemon job per instance and command (AGT-019) |
 * | `useToolRun(id \| { match, threadId? })` | a tool run of a thread with its live output (AGT-020) |
 * | `useTopic(topic)` | any topic's subscription state, followed while mounted |
 * | `useSnapshot(select)` | anything else from the per-frame model snapshot |
 * | `useCall()` | `call(method, params)`: every protocol method, typed |
 * | `useWindowState(key)` | window-local state by {@link WindowKey} (`SELECTED_THREAD`, `draftKey(id)`, …) |
 * | `useCommand(command)` / `useRunCommand()` | commands for the keymap and the `:` line (UI-095) |
 * | `useAnnounce()` | the ARIA live region (UI-092) |
 * | `useThreadActions()` | select, draft threads (open, discard, send the first message), archive/unarchive, delete (`requestDelete` asks first), rename, stop, restart; archiving or deleting forgets the selection and the draft (UI-023, UI-026) |
 * | `useReportError()` | puts a failure in the frame's error list (`ERRORS`) |
 * | `useCurrentWorkspaceId()` | the workspace this window works in: the selected thread's, else the draft thread's |
 * | `usePathSources()` | `suggest`/`complete` for the kit's `PathField`, on the daemon's machine |
 * | `useHost()` | the host itself (registry, window store), for frame-level code |
 * | `useSettings()` | the daemon's UI settings (`ui.density`, `ui.fontSize`, `ui.notifications`, `ui.fonts`, `keymap`, `error`), live (UI-097) |
 * | `useSetSettings()` | `settings.set(patch)`: merges `ui`/`keymap` into the config file; failures go to `ERRORS` |
 *
 * Shared window keys: `SELECTED_THREAD`, `draftKey(id)`, `DRAFT_THREAD` and `DRAFT_MESSAGE` (the draft thread),
 * `COLOR_SCHEME` (persisted, applied to `<html>` by the frame), `ACTIVE_VIEW`, `DELETE_THREAD`, `DELETE_WORKSPACE`,
 * `RENAMING_THREAD`, `ERRORS`. A widget is a
 * {@link WidgetComponent} (`ComponentType<WidgetProps>`); its root carries `data-omfp-scope` to own scoped commands.
 */

// View models, strings and helpers the built-in widgets use (AGT-016, AGT-021): a copied built-in imports them from
// `omfp/sdk` like any agent widget.
export { useVirtualizer, type VirtualItem } from "@tanstack/react-virtual";
export { strings } from "../strings/index.ts";
export {
  alternative,
  behavior,
  canAbort,
  commandsOf,
  complete,
  INTERRUPT_WINDOW_MS,
  imageAttachment,
  interruptAction,
  isBlank,
  isBusy,
  keyAction,
  type Modifiers,
  restoreCleared,
  restoreDraft,
  restoredTexts,
  restoreFailedPrompt,
  type SendMode,
  type Suggestion,
  sendMode,
  slashSuggestions,
  steerModifier,
} from "../vm/composer.ts";
export {
  decodeModels,
  decodeSession,
  decodeThinkingLevels,
  type HeaderView,
  headerView,
  NO_DETAILS,
  splitModel,
  type ThreadDetails,
} from "../vm/header.ts";
export {
  type AskForm,
  cardKey,
  checkAsk,
  closedRows,
  type Dialog as InteractionDialog,
  emptyAskForm,
  formatCountdown,
  isDialog,
  remainingMs,
  setCustom,
  timeoutMs,
  title as interactionTitle,
  toggleOption,
} from "../vm/interaction.ts";
export { type NoticeRow, noticeRow } from "../vm/notice.ts";
export {
  type PendingQueueOp,
  type QueueChip,
  type QueueOp,
  queueChips,
  queueOpApplied,
  settleQueueOp,
} from "../vm/queue.ts";
export { type SystemRow, systemMessage, systemNotice } from "../vm/system.ts";
export {
  type RepoKind,
  statusGlyph,
  statusLabel,
  type ThreadStatus,
  type WorkspaceChip,
  workspaceChips,
} from "../vm/threads.ts";
export { expandedByDefault, formatDuration, resultText, type ToolCard } from "../vm/tool.ts";
export {
  type Block as MessageBlock,
  type Item as TranscriptItem,
  items as transcriptItems,
  type MessageView,
  messageView,
  type Role as MessageRole,
  streamingView,
} from "../vm/transcript.ts";
export {
  INITIAL_PIN,
  isScrollUpKey,
  keyScrollTop,
  mountScrollTop,
  type NavCommand,
  navTarget,
  type PinEvent,
  type PinState,
  pinReducer,
} from "../vm/virtual.ts";
export {
  ADD_KINDS,
  type AddKind,
  addRepoSource,
  type RepoRow,
  removalNeedsConfirmation,
  type WorkspaceRow,
  workspaceRow,
} from "../vm/workspaces.ts";
export { StatusBadge, WorkspaceChipView } from "../widgets/threads/badges.tsx";
export {
  currentWorkspaceId,
  errorMessage,
  removeWorkspace,
  reportError,
  type ThreadActions,
  threadActions,
  useCurrentWorkspaceId,
  useReportError,
  useThreadActions,
} from "./actions.ts";
export { type Command, CommandRegistry, SCOPE_ATTRIBUTE, scopesAt } from "./commands.ts";
export {
  animationFrame,
  emptyDirty,
  type FrameScheduler,
  FrameStore,
  type ModelSource,
  type Snapshot,
} from "./frame-store.ts";
export {
  useAnnounce,
  useCall,
  useCommand,
  useConnection,
  useIndex,
  useRunCommand,
  useSnapshot,
  useThread,
  useThreadSummary,
  useThreads,
  useTopic,
  useViews,
  useWindowState,
  useWorkspace,
  useWorkspaces,
  type WorkspaceData,
} from "./hooks.ts";
export {
  type Announcement,
  Announcer,
  createHost,
  type HostOptions,
  type OmfpHost,
  OmfpProvider,
  type SdkClient,
  TopicRefs,
  useHost,
} from "./host.tsx";
export {
  ACTIVE_VIEW,
  COLOR_SCHEME,
  type ColorScheme,
  DELETE_THREAD,
  DELETE_WORKSPACE,
  DRAFT_MESSAGE,
  DRAFT_THREAD,
  type Draft,
  type DraftThread,
  draftKey,
  EMPTY_DRAFT,
  ERRORS,
  RENAMING_THREAD,
  SELECTED_THREAD,
} from "./keys.ts";
export {
  browserImporter,
  type Importer,
  type LoadedWidget,
  type WidgetLoadError,
  WidgetModules,
} from "./modules.ts";
export { type PathSources, usePathSources } from "./paths.ts";
export {
  DEFAULT_DENSITY,
  DEFAULT_FONT_SIZE,
  DEFAULT_NOTIFICATIONS,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  useSetSettings,
  useSettings,
} from "./settings.ts";
export {
  type Job,
  type StoredUpdate,
  type ToolRunInfo,
  type ToolRunTarget,
  useJob,
  useStored,
  useToolRun,
} from "./sources.ts";
export {
  useWidget,
  type WidgetComponent,
  type WidgetInfoScope,
  type WidgetProps,
  WidgetScope,
} from "./widget.ts";
export { type KeyValueStorage, type Update, type WindowKey, WindowStore, windowKey } from "./window-store.ts";
