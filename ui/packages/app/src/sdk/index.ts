/**
 * `@omfp/app/sdk`: what widgets build on (the seed of M4's `omfp/sdk`, docs/specs/08-agent-control.md).
 *
 * A window creates one {@link OmfpHost} over its protocol client ({@link createHost}) and provides it with
 * {@link OmfpProvider}; widgets then use only the hooks:
 *
 * | Hook | Gives |
 * | --- | --- |
 * | `useConnection()` | connection status (connecting, connected, reconnecting, …) |
 * | `useIndex()`, `useThreads()`, `useThreadSummary(id)`, `useWorkspaces()`, `useRepositories()` | the `index` topic |
 * | `useThread(id)` | the thread's `ThreadView` (reducer state + closed interactions), following `thread/<id>` |
 * | `useWorkspace(id)` | the workspace and its repositories' git status, following `workspace/<id>` |
 * | `useTopic(topic)` | any topic's subscription state, followed while mounted |
 * | `useSnapshot(select)` | anything else from the per-frame model snapshot |
 * | `useCall()` | `call(method, params)`: every protocol method, typed |
 * | `useWindowState(key)` | window-local state by {@link WindowKey} (`SELECTED_THREAD`, `draftKey(id)`, …) |
 * | `useCommand(command)` / `useRunCommand()` | commands for the keymap and the `:` line (UI-095) |
 * | `useAnnounce()` | the ARIA live region (UI-092) |
 * | `useThreadActions()` | select, archive/unarchive, delete (`requestDelete` asks first), rename, tags, stop, restart; archiving or deleting forgets the selection and the draft (UI-026) |
 * | `useReportError()` | puts a failure in the frame's error list (`ERRORS`) |
 * | `useHost()` | the host itself (registry, window store), for frame-level code |
 * | `useSettings()` | the daemon's UI settings (`ui.density`, `ui.notifications`, `ui.fonts`, `keymap`, `error`), live (UI-097) |
 * | `useSetSettings()` | `settings.set(patch)`: merges `ui`/`keymap` into the config file; failures go to `ERRORS` |
 *
 * Shared window keys: `SELECTED_THREAD`, `draftKey(id)`, `COLOR_SCHEME` (persisted, applied to `<html>` by the frame),
 * `ACTIVE_VIEW`, `NEW_THREAD_DIALOG`, `DELETE_THREAD`, `RENAMING_THREAD`, `ERRORS`. A widget is a
 * {@link WidgetComponent} (`ComponentType<WidgetProps>`); its root carries `data-omfp-scope` to own scoped commands.
 */

export {
  errorMessage,
  reportError,
  type ThreadActions,
  threadActions,
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
  useRepositories,
  useRunCommand,
  useSnapshot,
  useThread,
  useThreadSummary,
  useThreads,
  useTopic,
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
  type Draft,
  draftKey,
  EMPTY_DRAFT,
  ERRORS,
  NEW_THREAD_DIALOG,
  RENAMING_THREAD,
  SELECTED_THREAD,
} from "./keys.ts";
export { DEFAULT_DENSITY, DEFAULT_NOTIFICATIONS, useSetSettings, useSettings } from "./settings.ts";
export type { WidgetComponent, WidgetProps } from "./widget.ts";
export { type KeyValueStorage, type Update, type WindowKey, WindowStore, windowKey } from "./window-store.ts";
