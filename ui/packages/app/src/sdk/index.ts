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
 * | `useJob(command, { key? })` | `{ status, lines, stdout, stderr, exitCode, signal, runs, startedAt, endedAt, restart, kill }`: one daemon job per instance and command, or per key (AGT-019, AGT-086) |
 * | `useToolRun(id \| { match, threadId? })` | a tool run of a thread with its live output (AGT-020) |
 * | `useSlotReport()` | the report of the module widget slot being rendered (`widgets.report`, AGT-026); `undefined` in built-ins |
 * | `useTopic(topic)` | any topic's subscription state, followed while mounted |
 * | `useSnapshot(select)` | anything else from the per-frame model snapshot |
 * | `useCall()` | `call(method, params)`: every protocol method, typed |
 * | `useWindowState(key)` | window-local state by {@link WindowKey} (`SELECTED_THREAD`, `draftKey(id)`, …) |
 * | `useCommand(command)` / `useRunCommand()` | commands for the keymap and the `:` line (UI-095); `registerCommand(host, command)` outside React |
 * | `useAnnounce()` | the ARIA live region (UI-092) |
 * | `useThreadActions()` | select, draft threads (open, discard, send the first message), archive/unarchive, delete (`requestDelete` asks first), rename, stop, restart; archiving or deleting forgets the selection and the draft (UI-023, UI-026) |
 * | `useReportError()` | puts a failure in the frame's error list (`ERRORS`) |
 * | `useUrlActions()` | `open(url)` in a new tab, `copy(url)` to the clipboard: the URLs omp opened (UI-346) |
 * | `useCurrentWorkspaceId()` | the workspace this window works in: the selected thread's, else the draft thread's |
 * | `usePathSources()` | `suggest`/`complete` for the kit's `PathField`, on the daemon's machine |
 * | `useHost()` | the host itself (registry, window store), for frame-level code |
 * | `useSettings()` | the daemon's UI settings (`ui.density`, `ui.fontSize`, `ui.motion`, `ui.notifications`, `ui.fonts`, `keymap`, `error`), live (UI-097) |
 * | `useSetSettings()` | `settings.set(patch)`: merges `ui`/`keymap` into the config file; failures go to `ERRORS` |
 * | `useFile(path, { follow? })` | a workspace file (`workspaces.read_file`), followed, with `reload` and `save(content, expectedHash?)` (AGT-027) |
 * | `useOpenFile()` | `openFile(path, { line?, diff? })`: opens a file tab (UI-115) |
 * | `useMissingFile(path, missing, options)` | finds the file a path that does not exist names (`workspaces.find_file`): one opens in its place, several open the frame's picker (UI-117) |
 * | `useFileTabs(workspaceId)` | this window's file tabs of a workspace, `select` and `close` |
 * | `useLive()` | `[live, snapshot]`: this window's live voice session (start, end, mute, spectrum source, UI-180..UI-189) |
 * | `useNotify()` | `notify({ title, body, threadId? })`: a notice in this window's mode line, a system notification while the window has no focus (UI-194) |
 *
 * Init files (UI-260..UI-263, UI-267) are `export default function init(omfp: InitApi)`: the
 * {@link InitApi} gives `command`, `bind`, `notify`, `on`, `views` (a {@link ViewsApi}: `load`, `use`, `transform` over
 * {@link InitViews} and {@link Layout}), `workspace`, `panel`, `run`, `call`, `onCleanup` and the `host` itself; the
 * window's {@link InitFiles} runs them. TSX views (UI-268) return the layout components `Row`, `Col`, `Tabs`, `Stack`
 * and `Widget`.
 *
 * Shared window keys: `SELECTED_THREAD`, `draftKey(id)`, `DRAFT_THREAD` and `DRAFT_MESSAGE` (the draft thread),
 * `COLOR_SCHEME` (persisted, applied to `<html>` by the frame), `ACTIVE_VIEW` (persisted), `DELETE_THREAD`,
 * `DELETE_WORKSPACE`,
 * `RENAMING_THREAD`, `FOCUS_COMPOSER`, `ERRORS`, `OPEN_FILES`, `FLOATING_FILES` and `floatingWidgetKey(workspaceId, panelId)`
 * (persisted), `FILE_PICKER`, `EDITOR_MODE`, `NOTICES`.
 * A widget is a {@link WidgetComponent} (`ComponentType<WidgetProps>`); its root carries `data-omfp-scope` to own
 * scoped commands.
 */

// View models, strings and helpers the built-in widgets use (AGT-016, AGT-021): a copied built-in imports them from
// `omfp/sdk` like any agent widget.
export { useVirtualizer, type VirtualItem } from "@tanstack/react-virtual";
export { keyLabel } from "../keymap/keymap.ts";
export { strings } from "../strings/index.ts";
export { type AnsiRun, ansiRuns } from "../vm/ansi.ts";
export {
  type BackgroundRow,
  backgroundRows,
  type JobTimes,
  lastOutputLine,
  observeJobs,
  runningInBackground,
  stopJobMessage,
} from "../vm/background.ts";
export {
  type ChangedFile,
  type ChangesRepo,
  type ChangesSections,
  changeRunsKey,
  changesSections,
  changesView,
} from "../vm/changes.ts";
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
  closeTab,
  type FileTab,
  type FileTabs,
  FLOATING_MIN,
  FLOATING_PANEL_DEFAULT,
  type FloatingPanel,
  fileName,
  fileTabId,
  movePanel,
  NO_FILE_TABS,
  type OpenFileOptions,
  type OpenFiles,
  openTab,
  originalFromPatch,
  resizePanel,
  retargetTab,
  workspaceFilePath,
} from "../vm/files.ts";
export {
  costLabel,
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
  type HistoryStep,
  type HistoryWalk,
  historyDown,
  historyUp,
  onFirstLine,
  onLastLine,
} from "../vm/history.ts";
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
export {
  IDLE as LIVE_IDLE,
  type LivePanelView,
  type LiveSupport,
  type LocalLive,
  liveLines,
  livePanelView,
  type MicButton,
  micButton,
  ownStatus,
} from "../vm/live.ts";
export { markdownChunks } from "../vm/markdown.ts";
export {
  MCP_IDLE,
  type McpMark,
  type McpPending,
  type McpRow,
  mcpRows,
  mcpStatus,
  reauthPrompt,
} from "../vm/mcp.ts";
export { type NoticeRow, noticeRow } from "../vm/notice.ts";
export { OpenedUrlWatch, openedUrls, threadOpenedUrls, urlHost } from "../vm/opened-url.ts";
export {
  applyEdit,
  type Edited,
  editPaste,
  expandPastes,
  insertPaste,
  isLargePaste,
  isRawPaste,
  lineCount,
  rawPasteKeys,
  removePaste,
} from "../vm/paste.ts";
export {
  type PendingQueueOp,
  type QueueChip,
  type QueueOp,
  queueChips,
  queueOpApplied,
  settleQueueOp,
} from "../vm/queue.ts";
export {
  appendTranscriptPage,
  EMPTY_TRANSCRIPT,
  type SubagentRow,
  type SubagentTranscript,
  subagentRows,
  type TranscriptLine as SubagentLine,
  transcriptLines as subagentLines,
} from "../vm/subagents.ts";
export { type SystemRow, systemMessage, systemNotice } from "../vm/system.ts";
export {
  type RepoKind,
  statusGlyph,
  statusLabel,
  type ThreadStatus,
  type WorkspaceChip,
  workspaceChip,
} from "../vm/threads.ts";
export { type GoalView, type TodoPhaseView, type TodoRow, type TodosView, todosView } from "../vm/todos.ts";
export { expandedByDefault, formatDuration, resultText, type ToolCard } from "../vm/tool.ts";
export {
  type RepoPath,
  repoPath,
  type SubagentRun,
  type TodoPhase,
  type TodoTask,
  type ToolView,
} from "../vm/tool-view.ts";
export {
  type Block as MessageBlock,
  type Item as TranscriptItem,
  items as transcriptItems,
  type MessageView,
  messageView,
  type Role as MessageRole,
  type RowContent as TranscriptRowContent,
  rowContent as transcriptRowContent,
  streamingView,
} from "../vm/transcript.ts";
export {
  type NavigateOutcome,
  navigateOutcome,
  type TreeRow,
  type TreeView,
  treeView,
  withEditorText,
} from "../vm/tree.ts";
export {
  INITIAL_PIN,
  isScrollUpKey,
  keyScrollTop,
  mountScrollTop,
  movedUpByUser,
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
export { StatusBadge } from "../widgets/threads/badges.tsx";
export {
  currentWorkspaceId,
  errorMessage,
  removeWorkspace,
  reportError,
  type ThreadActions,
  threadActions,
  type UrlActions,
  useCurrentWorkspaceId,
  useReportError,
  useThreadActions,
  useUrlActions,
} from "./actions.ts";
export { type Background, useBackground, useRunningInBackground, useSubagentRows } from "./background.ts";
export { type Command, CommandRegistry, SCOPE_ATTRIBUTE, scopesAt } from "./commands.ts";
export {
  cancelFilePicker,
  chooseFile,
  EDITOR_MODE,
  errorCode,
  FILE_PICKER,
  FILE_POLL_MS,
  type FileLoadStatus,
  type FilePicker,
  type FilePoll,
  type FileState,
  type FileTabsState,
  FLOATING_FILES,
  findMissingFile,
  floatingWidgetKey,
  intervalPoll,
  type MissingOutcome,
  OPEN_FILES,
  openFile,
  type SaveResult,
  useFile,
  useFileTabs,
  useMissingFile,
  useOpenFile,
} from "./files.ts";
export {
  animationFrame,
  emptyDirty,
  type FrameScheduler,
  FrameStore,
  type ModelSource,
  type Snapshot,
} from "./frame-store.ts";
export {
  registerCommand,
  useAnnounce,
  useCall,
  useCommand,
  useConnection,
  useIndex,
  useLive,
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
  RECENT_THREADS,
  type SdkClient,
  TopicRefs,
  useHost,
} from "./host.tsx";
export {
  type InitApi,
  type InitBinding,
  type InitCommand,
  type InitContext,
  type InitError,
  type InitFile,
  InitFiles,
  type InitRepo,
  type InitSnapshot,
  type InitWorkspace,
  type ThreadEventKind,
  type ViewsApi,
} from "./init.ts";
export { InitViews, Layout, type ViewEntry, type ViewOrigin, type ViewSpec } from "./init-views.ts";
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
  FOCUS_COMPOSER,
  OPEN_SUBAGENT,
  type OpenSubagentRequest,
  type Paste,
  RENAMING_THREAD,
  REVEAL_WIDGET,
  type RevealRequest,
  SELECTED_THREAD,
} from "./keys.ts";
export {
  Col,
  type ContainerElementProps,
  Row,
  Stack,
  Tabs,
  type TabsElementProps,
  Widget,
  type WidgetElementProps,
} from "./layout.tsx";
export {
  browserLiveMedia,
  LiveClient,
  type LiveClientOptions,
  type LiveLink,
  type LiveMedia,
  LiveOpenError,
  type LiveSnapshot,
} from "./live.ts";
export {
  browserImporter,
  type Importer,
  type LoadedWidget,
  type WidgetLoadError,
  WidgetModules,
} from "./modules.ts";
export {
  NOTICE_MS,
  NOTICES,
  type Notice,
  type NoticeRequest,
  type NoticeRoute,
  Notices,
  type NoticeTimers,
  type NoticeTone,
  useNotify,
} from "./notify.ts";
export { type PathSources, usePathSources } from "./paths.ts";
export {
  JOB_TAIL,
  REPORT_DELAY_MS,
  type ReportSink,
  type ReportTimers,
  type SlotIdentity,
  type SlotLoad,
  SlotReport,
  SlotReportScope,
  useSlotReport,
  WidgetReporter,
} from "./reports.ts";
export {
  DEFAULT_DENSITY,
  DEFAULT_FONT_SIZE,
  DEFAULT_MOTION,
  DEFAULT_NOTIFICATIONS,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  useSetSettings,
  useSettings,
} from "./settings.ts";
export {
  type Job,
  type JobOptions,
  type JobRunStatus,
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
