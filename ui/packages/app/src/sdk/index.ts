/**
 * `omfp/sdk` (AGT-016): what every widget builds on, built-in or written by an agent (docs/specs/08-agent-control.md);
 * the bundle exposes this module to widget modules through the page's import map (AGT-012).
 *
 * A window creates one {@link OmfpHost} over its protocol client ({@link createHost}) and provides it with
 * {@link OmfpProvider}; widgets then use only the hooks.
 *
 * `crates/omfp-daemon/src/widgets/docs.md` (`omfp://docs/widgets`) is the reference of every hook and component agents
 * use: keep it complete there.
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
  queueKeys,
  queueModifier,
  restoreCleared,
  restoreDraft,
  restoredTexts,
  restoreFailedPrompt,
  type SendMode,
  type Suggestion,
  sendMode,
  slashSuggestions,
} from "../vm/composer.ts";
export {
  closeOpenedTab,
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
  openedTabIds,
  openTab,
  openWidgetTab,
  originalFromPatch,
  resizePanel,
  retargetTab,
  widgetTabId,
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
export {
  loginCompletions,
  logoutCompletions,
  logoutSummary,
  needsLogin,
  providerName,
  withAuthenticated,
} from "../vm/login.ts";
export { markdownChunks } from "../vm/markdown.ts";
export {
  MCP_IDLE,
  type McpMark,
  type McpPending,
  type McpRow,
  mcpRows,
  mcpStatus,
  type PendingRestart,
  pendingRestart,
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
export { type CardImage, expandedByDefault, formatDuration, resultText, type ToolCard } from "../vm/tool.ts";
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
export type { ThreadViewChoice, ViewChoices } from "../vm/view-choices.ts";
export {
  arrivalScroll,
  halfPageScrollTop,
  INITIAL_PIN,
  type ItemEdges,
  type ItemExtent,
  isScrollUpKey,
  keyScrollTop,
  type MoveCommand,
  mountScrollTop,
  movedUpByUser,
  type NavCommand,
  navTarget,
  type PinEvent,
  type PinState,
  pagerScroll,
  pinReducer,
  refocusAfterScroll,
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
  reportSuccess,
  requestLogin,
  type ThreadActions,
  threadActions,
  type UrlActions,
  useCurrentWorkspaceId,
  useReportError,
  useReportSuccess,
  useThreadActions,
  useUrlActions,
} from "./actions.ts";
export { chooseView, useActiveView, useViewChoicesUpkeep } from "./active-view.ts";
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
  type CallState,
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
  AUTH_REVISION,
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
  LOGIN,
  type LoginRequest,
  OPEN_SUBAGENT,
  type OpenSubagentRequest,
  type Paste,
  RENAMING_THREAD,
  RESET_VIEWS,
  REVEAL_WIDGET,
  type RevealRequest,
  SELECTED_THREAD,
  SUCCESSES,
  type Success,
  VIEW_CHOICES,
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
  MESSAGES,
  MESSAGES_KEPT,
  type Message,
  type MessageTone,
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
export { jsonSize, RESULT_ENTRIES, RESULT_UNITS, ResultCache, resultKey } from "./results.ts";
export {
  DEFAULT_DENSITY,
  DEFAULT_FONT_SIZE,
  DEFAULT_MOTION,
  DEFAULT_NOTIFICATIONS,
  DEFAULT_THEME,
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
