//! Entities the daemon exposes: repositories, workspaces, thread summaries, git status and terminals.
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

/// A known git repository: a shortcut for adding repositories to workspaces (DMN-021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: String,
    pub name: String,
    /// Absolute path of the repository root.
    pub path: String,
    pub default_base_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    pub created_at: i64,
}

/// A folder threads work in; its repositories are its top-level entries (D-11, DMN-020).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    /// Unique among workspaces.
    pub name: String,
    /// Absolute path of the folder, unique among workspaces.
    pub path: String,
    /// The daemon created the folder and may delete it (DMN-028).
    pub owned: bool,
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
    pub title: String,
    pub tags: Vec<String>,
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

/// State of the `index` topic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IndexSnapshot {
    pub repositories: Vec<Repository>,
    pub workspaces: Vec<Workspace>,
    pub threads: Vec<ThreadSummary>,
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
