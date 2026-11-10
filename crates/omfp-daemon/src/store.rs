//! SQLite persistence (02-daemon, "Persistence"): repositories, workspaces, threads, tags and settings.
//!
//! Transcripts are not stored here (DMN-012): omp's session files in each thread's `session_dir` are the source of
//! truth. All calls are blocking; the server layer runs them on blocking tasks.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};

/// Embedded migrations; migration `i` brings the schema to `user_version = i + 1` (DMN-010).
const MIGRATIONS: &[&str] = &[
    include_str!("store/0001_initial.sql"),
    include_str!("store/0002_created_client.sql"),
];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{entity} {id} not found")]
    NotFound { entity: &'static str, id: String },
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(
        "database schema version {found} is not supported (this daemon knows up to {supported})"
    )]
    UnknownSchemaVersion { found: i64, supported: i64 },
    #[error("path is not valid UTF-8: {}", .0.display())]
    InvalidPath(PathBuf),
    #[error("stored value is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

/// Who created an entity (ARC-009): the user, or the agent of a thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    User,
    Thread(String),
}

impl Provenance {
    /// Stored form: `user` or the thread id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::User => "user",
            Self::Thread(id) => id,
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Self {
        if value == "user" {
            Self::User
        } else {
            Self::Thread(value.to_owned())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repository {
    pub id: String,
    pub name: String,
    /// Absolute path of the repository root.
    pub path: PathBuf,
    pub default_base_ref: String,
    pub remote_url: Option<String>,
    /// Unix milliseconds.
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceKind {
    /// A git worktree created (and removable) by the daemon.
    ManagedWorktree,
    /// An existing directory the daemon never deletes (DMN-023).
    Adopted,
}

impl WorkspaceKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManagedWorktree => "managed_worktree",
            Self::Adopted => "adopted",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "managed_worktree" => Some(Self::ManagedWorktree),
            "adopted" => Some(Self::Adopted),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub id: String,
    /// Absolute path of the directory.
    pub path: PathBuf,
    pub kind: WorkspaceKind,
    pub repository_id: Option<String>,
    pub branch: Option<String>,
    pub base_ref: Option<String>,
    pub pr_url: Option<String>,
    pub pr_number: Option<i64>,
    pub created_by: Provenance,
    /// Unix milliseconds.
    pub created_at: i64,
    /// Set when the workspace was removed or forgotten; removed workspaces are no longer listed.
    pub removed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub session_dir: PathBuf,
    pub omp_session_id: Option<String>,
    pub approval_mode: Option<String>,
    /// `provider/id`.
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub created_by: Provenance,
    /// Unix milliseconds.
    pub created_at: i64,
    pub archived_at: Option<i64>,
    /// Attached workspaces in order (the first one comes first in `--add-dir`).
    pub workspace_ids: Vec<String>,
    /// Sorted and deduplicated.
    pub tags: Vec<String>,
}

/// A new ULID as a string. IDs are monotonic within the process, so ordering by id is ordering by creation.
#[must_use]
pub fn new_id() -> String {
    static GENERATOR: Mutex<ulid::Generator> = Mutex::new(ulid::Generator::new());
    let mut generator = GENERATOR.lock().unwrap_or_else(PoisonError::into_inner);
    generator
        .generate()
        .unwrap_or_else(|_| ulid::Ulid::generate())
        .to_string()
}

/// The current time in Unix milliseconds.
#[must_use]
pub fn now_ms() -> i64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
}

/// The daemon database. One connection behind a mutex; every method is a short blocking call.
#[derive(Debug)]
pub struct Store {
    conn: Mutex<Connection>,
}

const REPOSITORY_COLUMNS: &str = "id, name, path, default_base_ref, remote_url, created_at";
const WORKSPACE_COLUMNS: &str = "id, path, kind, repository_id, branch, base_ref, pr_url, pr_number, created_by, \
     created_at, removed_at";
const THREAD_COLUMNS: &str = "id, title, session_dir, omp_session_id, approval_mode, model, thinking_level, \
     created_by, created_at, archived_at";

impl Store {
    /// Opens (creating if needed) the database at `path`, enables WAL and foreign keys (DMN-011) and applies pending
    /// migrations (DMN-010).
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock leaves no open transaction behind (rolled back on drop).
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    // Repositories

    pub fn insert_repository(&self, repo: &Repository) -> Result<()> {
        self.conn()
            .execute(
                "INSERT INTO repositories (id, name, path, default_base_ref, remote_url, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    repo.id,
                    repo.name,
                    path_str(&repo.path)?,
                    repo.default_base_ref,
                    repo.remote_url,
                    repo.created_at
                ],
            )
            .map_err(|error| {
                unique_conflict(
                    error,
                    &format!("repository {} is already registered", repo.path.display()),
                )
            })?;
        Ok(())
    }

    pub fn repository(&self, id: &str) -> Result<Repository> {
        self.conn()
            .query_row(
                &format!("SELECT {REPOSITORY_COLUMNS} FROM repositories WHERE id = ?1"),
                [id],
                repository_from_row,
            )
            .optional()?
            .ok_or_else(|| not_found("repository", id))
    }

    pub fn repository_by_path(&self, path: &Path) -> Result<Option<Repository>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {REPOSITORY_COLUMNS} FROM repositories WHERE path = ?1"),
                [path_str(path)?],
                repository_from_row,
            )
            .optional()?)
    }

    pub fn repositories(&self) -> Result<Vec<Repository>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {REPOSITORY_COLUMNS} FROM repositories ORDER BY id"
        ))?;
        let rows = stmt.query_map([], repository_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Unregisters a repository; its workspaces stay, without a repository.
    pub fn delete_repository(&self, id: &str) -> Result<()> {
        let deleted = self
            .conn()
            .execute("DELETE FROM repositories WHERE id = ?1", [id])?;
        if deleted == 0 {
            return Err(not_found("repository", id));
        }
        Ok(())
    }

    // Workspaces

    pub fn insert_workspace(&self, ws: &Workspace) -> Result<()> {
        self.conn()
            .execute(
                "INSERT INTO workspaces (id, path, kind, repository_id, branch, base_ref, pr_url, pr_number,
                     created_by, created_at, removed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    ws.id,
                    path_str(&ws.path)?,
                    ws.kind.as_str(),
                    ws.repository_id,
                    ws.branch,
                    ws.base_ref,
                    ws.pr_url,
                    ws.pr_number,
                    ws.created_by.as_str(),
                    ws.created_at,
                    ws.removed_at
                ],
            )
            .map_err(|error| {
                unique_conflict(
                    error,
                    &format!("{} is already a workspace", ws.path.display()),
                )
            })?;
        Ok(())
    }

    /// A workspace by id, removed or not.
    pub fn workspace(&self, id: &str) -> Result<Workspace> {
        self.conn()
            .query_row(
                &format!("SELECT {WORKSPACE_COLUMNS} FROM workspaces WHERE id = ?1"),
                [id],
                workspace_from_row,
            )
            .optional()?
            .ok_or_else(|| not_found("workspace", id))
    }

    pub fn live_workspace_by_path(&self, path: &Path) -> Result<Option<Workspace>> {
        Ok(self
            .conn()
            .query_row(
                &format!(
                    "SELECT {WORKSPACE_COLUMNS} FROM workspaces WHERE path = ?1 AND removed_at IS NULL"
                ),
                [path_str(path)?],
                workspace_from_row,
            )
            .optional()?)
    }

    /// Live (not removed) workspaces.
    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(&format!(
            "SELECT {WORKSPACE_COLUMNS} FROM workspaces WHERE removed_at IS NULL ORDER BY id"
        ))?;
        let rows = stmt.query_map([], workspace_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Marks a workspace removed and detaches it from every thread, in one transaction.
    pub fn mark_workspace_removed(&self, id: &str, removed_at: i64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let updated = tx.execute(
            "UPDATE workspaces SET removed_at = ?2 WHERE id = ?1",
            params![id, removed_at],
        )?;
        if updated == 0 {
            return Err(not_found("workspace", id));
        }
        let affected: Vec<String> = {
            let mut stmt =
                tx.prepare("SELECT thread_id FROM thread_workspaces WHERE workspace_id = ?1")?;
            stmt.query_map([id], |row| row.get(0))?
                .collect::<Result<_, _>>()?
        };
        tx.execute(
            "DELETE FROM thread_workspaces WHERE workspace_id = ?1",
            [id],
        )?;
        for thread_id in &affected {
            let remaining = load_workspace_ids(&tx, thread_id)?;
            write_workspace_ids(&tx, thread_id, &remaining)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Ids of the threads a workspace is attached to (DMN-027), oldest thread first.
    pub fn workspace_thread_ids(&self, workspace_id: &str) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT t.id FROM thread_workspaces tw JOIN threads t ON t.id = tw.thread_id
             WHERE tw.workspace_id = ?1 ORDER BY t.id",
        )?;
        let rows = stmt.query_map([workspace_id], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    // Threads

    /// Inserts a thread with its workspaces and tags, in one transaction.
    pub fn insert_thread(&self, thread: &Thread) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO threads (id, title, session_dir, omp_session_id, approval_mode, model, thinking_level,
                 created_by, created_at, archived_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                thread.id,
                thread.title,
                path_str(&thread.session_dir)?,
                thread.omp_session_id,
                thread.approval_mode,
                thread.model,
                thread.thinking_level,
                thread.created_by.as_str(),
                thread.created_at,
                thread.archived_at
            ],
        )
        .map_err(|error| unique_conflict(error, &format!("thread {} exists", thread.id)))?;
        write_workspace_ids(&tx, &thread.id, &thread.workspace_ids)?;
        write_tags(&tx, &thread.id, &thread.tags)?;
        tx.commit()?;
        Ok(())
    }

    pub fn thread(&self, id: &str) -> Result<Thread> {
        let conn = self.conn();
        let thread = conn
            .query_row(
                &format!("SELECT {THREAD_COLUMNS} FROM threads WHERE id = ?1"),
                [id],
                thread_from_row,
            )
            .optional()?
            .ok_or_else(|| not_found("thread", id))?;
        with_links(&conn, thread)
    }

    /// Every thread, archived ones included, oldest first.
    pub fn threads(&self) -> Result<Vec<Thread>> {
        let conn = self.conn();
        let threads: Vec<Thread> = {
            let mut stmt =
                conn.prepare(&format!("SELECT {THREAD_COLUMNS} FROM threads ORDER BY id"))?;
            stmt.query_map([], thread_from_row)?
                .collect::<Result<_, _>>()?
        };
        threads
            .into_iter()
            .map(|thread| with_links(&conn, thread))
            .collect()
    }

    pub fn set_thread_title(&self, id: &str, title: &str) -> Result<()> {
        self.update_thread(id, "UPDATE threads SET title = ?2 WHERE id = ?1", title)
    }

    pub fn set_thread_archived_at(&self, id: &str, archived_at: Option<i64>) -> Result<()> {
        self.update_thread(
            id,
            "UPDATE threads SET archived_at = ?2 WHERE id = ?1",
            archived_at,
        )
    }

    pub fn set_thread_omp_session_id(&self, id: &str, session_id: Option<&str>) -> Result<()> {
        self.update_thread(
            id,
            "UPDATE threads SET omp_session_id = ?2 WHERE id = ?1",
            session_id,
        )
    }

    pub fn set_thread_model(&self, id: &str, model: Option<&str>) -> Result<()> {
        self.update_thread(id, "UPDATE threads SET model = ?2 WHERE id = ?1", model)
    }

    pub fn set_thread_thinking_level(&self, id: &str, level: Option<&str>) -> Result<()> {
        self.update_thread(
            id,
            "UPDATE threads SET thinking_level = ?2 WHERE id = ?1",
            level,
        )
    }

    fn update_thread(&self, id: &str, sql: &str, value: impl rusqlite::ToSql) -> Result<()> {
        if self.conn().execute(sql, params![id, value])? == 0 {
            return Err(not_found("thread", id));
        }
        Ok(())
    }

    /// Replaces the thread's tags.
    pub fn set_thread_tags(&self, id: &str, tags: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        ensure_thread(&tx, id)?;
        tx.execute("DELETE FROM thread_tags WHERE thread_id = ?1", [id])?;
        write_tags(&tx, id, tags)?;
        tx.commit()?;
        Ok(())
    }

    /// Replaces the thread's ordered workspace list.
    pub fn set_thread_workspaces(&self, id: &str, workspace_ids: &[String]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        ensure_thread(&tx, id)?;
        write_workspace_ids(&tx, id, workspace_ids)?;
        tx.commit()?;
        Ok(())
    }

    /// Deletes a thread with its workspace links and tags; the workspaces themselves stay.
    pub fn delete_thread(&self, id: &str) -> Result<()> {
        if self
            .conn()
            .execute("DELETE FROM threads WHERE id = ?1", [id])?
            == 0
        {
            return Err(not_found("thread", id));
        }
        Ok(())
    }

    /// Checks that a provenance names an existing thread, if it names one (ARC-009).
    pub fn check_provenance(&self, created_by: &Provenance) -> Result<()> {
        match created_by {
            Provenance::User => Ok(()),
            Provenance::Thread(id) => ensure_thread(&self.conn(), id),
        }
    }

    /// Records the `client` of the connection that created thread or workspace `id` (PRT-033).
    pub fn set_created_client(&self, id: &str, client: &str) -> Result<()> {
        let conn = self.conn();
        let updated = conn.execute(
            "UPDATE threads SET created_client = ?2 WHERE id = ?1",
            params![id, client],
        )? + conn.execute(
            "UPDATE workspaces SET created_client = ?2 WHERE id = ?1",
            params![id, client],
        )?;
        if updated == 0 {
            return Err(not_found("thread or workspace", id));
        }
        Ok(())
    }

    /// The recorded creating clients of threads and workspaces, by id.
    pub fn created_clients(&self) -> Result<std::collections::HashMap<String, String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, created_client FROM threads WHERE created_client IS NOT NULL
             UNION ALL
             SELECT id, created_client FROM workspaces WHERE created_client IS NOT NULL",
        )?;
        let clients = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(clients)
    }

    // Settings

    pub fn setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let value: Option<String> = self
            .conn()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(value
            .map(|value| serde_json::from_str(&value))
            .transpose()?)
    }

    pub fn set_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.conn().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value.to_string()],
        )?;
        Ok(())
    }
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let found: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let supported = i64::try_from(MIGRATIONS.len()).unwrap_or(i64::MAX);
    let applied = usize::try_from(found)
        .ok()
        .filter(|&applied| applied <= MIGRATIONS.len())
        .ok_or(StoreError::UnknownSchemaVersion { found, supported })?;
    for (version, sql) in (1_i64..).zip(MIGRATIONS).skip(applied) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

fn not_found(entity: &'static str, id: &str) -> StoreError {
    StoreError::NotFound {
        entity,
        id: id.to_owned(),
    }
}

/// Maps unique/primary-key violations to [`StoreError::Conflict`]; other errors pass through.
fn unique_conflict(error: rusqlite::Error, message: &str) -> StoreError {
    match &error {
        rusqlite::Error::SqliteFailure(failure, _)
            if failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY =>
        {
            StoreError::Conflict(message.to_owned())
        }
        _ => StoreError::Sqlite(error),
    }
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| StoreError::InvalidPath(path.to_owned()))
}

fn ensure_thread(conn: &Connection, id: &str) -> Result<()> {
    conn.query_row("SELECT 1 FROM threads WHERE id = ?1", [id], |_| Ok(()))
        .optional()?
        .ok_or_else(|| not_found("thread", id))
}

fn write_workspace_ids(conn: &Connection, thread_id: &str, workspace_ids: &[String]) -> Result<()> {
    conn.execute(
        "DELETE FROM thread_workspaces WHERE thread_id = ?1",
        [thread_id],
    )?;
    let mut stmt = conn.prepare(
        "INSERT INTO thread_workspaces (thread_id, workspace_id, position) VALUES (?1, ?2, ?3)",
    )?;
    for (position, workspace_id) in (0_i64..).zip(workspace_ids) {
        stmt.execute(params![thread_id, workspace_id, position])
            .map_err(|error| {
                unique_conflict(error, &format!("workspace {workspace_id} is listed twice"))
            })?;
    }
    Ok(())
}

fn write_tags(conn: &Connection, thread_id: &str, tags: &[String]) -> Result<()> {
    let mut stmt =
        conn.prepare("INSERT OR IGNORE INTO thread_tags (thread_id, tag) VALUES (?1, ?2)")?;
    for tag in tags {
        stmt.execute(params![thread_id, tag])?;
    }
    Ok(())
}

fn load_workspace_ids(conn: &Connection, thread_id: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT workspace_id FROM thread_workspaces WHERE thread_id = ?1 ORDER BY position",
    )?;
    let rows = stmt.query_map([thread_id], |row| row.get(0))?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn with_links(conn: &Connection, mut thread: Thread) -> Result<Thread> {
    thread.workspace_ids = load_workspace_ids(conn, &thread.id)?;
    let mut stmt = conn.prepare("SELECT tag FROM thread_tags WHERE thread_id = ?1 ORDER BY tag")?;
    thread.tags = stmt
        .query_map([&thread.id], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    Ok(thread)
}

fn repository_from_row(row: &Row<'_>) -> rusqlite::Result<Repository> {
    Ok(Repository {
        id: row.get(0)?,
        name: row.get(1)?,
        path: PathBuf::from(row.get::<_, String>(2)?),
        default_base_ref: row.get(3)?,
        remote_url: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn workspace_from_row(row: &Row<'_>) -> rusqlite::Result<Workspace> {
    let kind: String = row.get(2)?;
    Ok(Workspace {
        id: row.get(0)?,
        path: PathBuf::from(row.get::<_, String>(1)?),
        kind: WorkspaceKind::parse(&kind).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                Type::Text,
                format!("unknown workspace kind {kind:?}").into(),
            )
        })?,
        repository_id: row.get(3)?,
        branch: row.get(4)?,
        base_ref: row.get(5)?,
        pr_url: row.get(6)?,
        pr_number: row.get(7)?,
        created_by: Provenance::parse(&row.get::<_, String>(8)?),
        created_at: row.get(9)?,
        removed_at: row.get(10)?,
    })
}

fn thread_from_row(row: &Row<'_>) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: row.get(0)?,
        title: row.get(1)?,
        session_dir: PathBuf::from(row.get::<_, String>(2)?),
        omp_session_id: row.get(3)?,
        approval_mode: row.get(4)?,
        model: row.get(5)?,
        thinking_level: row.get(6)?,
        created_by: Provenance::parse(&row.get::<_, String>(7)?),
        created_at: row.get(8)?,
        archived_at: row.get(9)?,
        workspace_ids: Vec::new(),
        tags: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: ARC-009
    #[test]
    fn provenance_is_stored_as_user_or_a_thread_id() {
        assert_eq!(Provenance::User.as_str(), "user");
        assert_eq!(Provenance::Thread("01J".into()).as_str(), "01J");
        assert_eq!(Provenance::parse("user"), Provenance::User);
        assert_eq!(Provenance::parse("01J"), Provenance::Thread("01J".into()));
    }

    #[test]
    fn workspace_kinds_round_trip() {
        for kind in [WorkspaceKind::ManagedWorktree, WorkspaceKind::Adopted] {
            assert_eq!(WorkspaceKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(WorkspaceKind::parse("other"), None);
    }
}
