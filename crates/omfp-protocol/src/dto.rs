//! Entities the daemon exposes: workspaces, thread summaries, git status, repository suggestions and terminals.
//!
//! Timestamps are Unix epoch milliseconds.

use omfp_core::ProcessState;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Who created an entity (ARC-009).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    pub created_by: CreatedBy,
    /// `client` of the creating connection's `system.hello` (PRT-033).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<String>,
}

/// The principal behind a created entity: the user, or the agent of a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CreatedBy {
    User,
    Thread {
        #[serde(rename = "threadId")]
        thread_id: String,
    },
}

/// A repository the daemon has seen in a workspace, suggested when adding repositories (DMN-021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SeenRepository {
    /// Canonical path: a link's target or a clone, the main repository of a worktree.
    pub path: String,
    /// The path's last component.
    pub name: String,
    pub last_seen_at: i64,
}

/// A child directory on the daemon host, offered while typing a path (DMN-021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PathEntry {
    pub name: String,
    /// Absolute path, `~` expanded.
    pub path: String,
    /// The directory is the root of a git repository.
    pub is_git_root: bool,
}

/// A folder threads work in; its repositories are its top-level entries (D-11, DMN-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    /// Unique among workspaces.
    pub name: String,
    /// Absolute path of the folder (under the daemon's workspaces root), unique among workspaces.
    pub path: String,
    /// Repositories discovered in the folder (DMN-022): the folder itself first when it is a git root, then the
    /// entries by name.
    pub repos: Vec<WorkspaceRepo>,
    /// Threads living in the workspace, archived ones included, oldest first (DMN-027).
    pub thread_ids: Vec<String>,
    pub provenance: Provenance,
    pub created_at: i64,
}

/// A repository discovered in a workspace folder (DMN-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRepo {
    /// Entry name in the folder; the folder's own name for the root.
    pub name: String,
    /// Path inside the workspace (the symlink path for a link, never its target).
    pub path: String,
    #[serde(flatten)]
    pub kind: RepoKind,
    /// `None` on a detached HEAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Commit id of HEAD; `None` before the first commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    /// The workspace folder itself is this repository.
    pub is_root: bool,
    /// The checkout with symlinks resolved: entries with the same canonical path are the same files (DMN-027).
    pub canonical_path: String,
    /// Other workspaces holding the same checkout (same canonical path), so clients can warn (DMN-027).
    pub shared_with: Vec<String>,
}

/// How a repository sits in its workspace folder (DMN-022).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepoKind {
    /// A symlink to a repository elsewhere.
    Link {
        /// The symlink's target, as written in the link.
        target: String,
    },
    /// A linked git worktree.
    Worktree {
        /// Root of the main working tree (`git rev-parse --git-common-dir`).
        main: String,
    },
    /// A repository of its own (a clone, or a folder initialized in place).
    Clone,
}

/// What the thread list shows about a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: String,
    /// `""` until the first message (DMN-073).
    pub title: String,
    pub title_source: TitleSource,
    /// The workspace the thread lives in: its omp cwd (ARC-008, DMN-029).
    pub workspace_id: String,
    /// `omfp_core::ProcessState`.
    #[schemars(with = "serde_json::Value")]
    pub process: ProcessState,
    /// Idle with nothing queued or pending (omp's `session_settled`).
    pub settled: bool,
    /// Interactive requests are waiting for an answer.
    pub needs_attention: bool,
    pub provenance: Provenance,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<i64>,
}

/// Where a thread's title comes from (DMN-073).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TitleSource {
    /// The first words of the first message (or `""` before it), until a title is generated or set.
    #[default]
    Placeholder,
    /// Generated from the conversation by the daemon.
    Generated,
    /// Set by the user or an agent (`threads.create`, `threads.update`, host tool `set_thread_title`); never replaced.
    Set,
}

/// The user's theme: CSS applied after the built-in tokens (DMN-075).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    /// `null`: the built-in themes only.
    #[schemars(required, extend("type" = ["string", "null"]))]
    pub css: Option<String>,
}

/// UI settings and key bindings the daemon keeps in its config file (DMN-009, UI-097).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub ui: UiSettings,
    /// Bindings over the defaults, in file order; `command: ""` removes a default binding.
    pub keymap: Vec<KeymapEntry>,
    /// Why the last reload of the config file failed (the previous settings stay); `null` when it is valid.
    #[schemars(required, extend("type" = ["string", "null"]))]
    pub error: Option<String>,
}

/// The `[ui]` table of the config file; an absent value is the default (UI-097).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<Density>,
    /// Root font size in px (10..=28, default 16); every rem-based size token scales with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 10, max = 28))]
    pub font_size: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notifications: Option<NotificationMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fonts: Option<FontSettings>,
}

/// Row height and spacing of the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    Compact,
    Normal,
}

/// When the UI shows system notifications (UI-096).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationMode {
    /// Only while the window has no focus (default).
    Background,
    Always,
    Never,
}

/// CSS `font-family` values overriding the font tokens; absent ones keep the theme's.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FontSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// One key binding of the config file's `[keymap]` (UI-097).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeymapEntry {
    pub mode: KeymapMode,
    /// Key sequence, e.g. `g g`, `Ctrl-d`, `Space n`.
    pub keys: String,
    /// Command id; `""` removes the default binding of `keys`.
    pub command: String,
}

/// The keymap mode a binding belongs to: `[keymap]`, `[keymap.insert]`, `[keymap.cmdline]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeymapMode {
    Normal,
    Insert,
    Cmdline,
}

/// State of the `index` topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IndexSnapshot {
    pub workspaces: Vec<Workspace>,
    pub threads: Vec<ThreadSummary>,
    pub theme: Theme,
    pub settings: Settings,
}

/// Git status of one repository of a workspace (the `workspace/<id>` topic, DMN-026).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RepoStatus {
    /// The repository's name in the workspace.
    pub name: String,
    pub status: GitStatus,
}

/// Git state of a repository relative to its upstream or base ref (DMN-026).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    /// `None` on a detached HEAD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// What `ahead`/`behind` count against: the branch's upstream; absent without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_ref: Option<String>,
    /// Commits on HEAD not in the base ref.
    pub ahead: u32,
    /// Commits in the base ref not on HEAD.
    pub behind: u32,
    /// Uncommitted changes, untracked files included.
    pub dirty: bool,
    /// Files changed since the base ref, committed or not.
    pub files: Vec<FileChange>,
}

/// One changed file with its line counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    /// Previous path of a renamed file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
}

/// Kind of change to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

/// One page of a workspace diff (PRT-013).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiffPage {
    pub files: Vec<FileDiff>,
    /// Pass as `cursor` to fetch the next page; absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Unified diff of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub patch: String,
    /// The patch was cut to fit the frame limit.
    pub truncated: bool,
}

/// What runs in a terminal (05-terminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalKind {
    /// Interactive omp on the thread's session.
    Omp,
    /// A login shell.
    Shell,
}

/// An open terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    pub kind: TerminalKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub cols: u16,
    pub rows: u16,
}
