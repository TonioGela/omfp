//! Embedded migrations (DMN-010), applied in order using `PRAGMA user_version`, each in its own transaction with
//! foreign keys checked before commit.
//!
//! Migration 3 (workspace folders, D-11) is not only SQL: every existing thread gets a new owned workspace folder
//! under the workspaces root, holding one symlink per directory of its old workspaces. Old worktrees stay where they
//! are (the symlinks point at them), and the old roots are recorded so the first start removes them from the session
//! (DMN-029).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, Transaction, params};
use tracing::{info, warn};

use super::{
    Provenance, Result, StoreError, Thread, Workspace, insert_workspace, new_id, path_str,
};
use crate::names::{slugify, unique};

const MIGRATION_1: &str = include_str!("0001_initial.sql");
const MIGRATION_2: &str = include_str!("0002_created_client.sql");
const MIGRATION_3: &str = include_str!("0003_workspace_folders.sql");

/// Number of migrations: the highest `user_version` this daemon knows.
const LATEST: i64 = 3;

/// Applies pending migrations. Foreign keys are off while migrating (tables are rebuilt) and checked before each
/// commit; the caller turns them on afterwards.
pub(super) fn run(conn: &mut Connection, workspaces_root: &Path) -> Result<()> {
    let found: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if !(0..=LATEST).contains(&found) {
        return Err(StoreError::UnknownSchemaVersion {
            found,
            supported: LATEST,
        });
    }
    conn.pragma_update(None, "foreign_keys", false)?;
    for version in (found + 1)..=LATEST {
        let tx = conn.transaction()?;
        match version {
            1 => tx.execute_batch(MIGRATION_1)?,
            2 => tx.execute_batch(MIGRATION_2)?,
            _ => to_workspace_folders(&tx, workspaces_root)?,
        }
        check_foreign_keys(&tx)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

fn check_foreign_keys(tx: &Transaction<'_>) -> Result<()> {
    let mut stmt = tx.prepare("PRAGMA foreign_key_check")?;
    let mut rows = stmt.query([])?;
    if let Some(row) = rows.next()? {
        let table: String = row.get(0)?;
        return Err(StoreError::Migration(format!(
            "foreign key violation in table {table}"
        )));
    }
    Ok(())
}

/// A thread as stored before migration 3, with the directories of its live workspaces in order.
struct OldThread {
    thread: Thread,
    client: Option<String>,
    dirs: Vec<OldDir>,
}

/// A directory of an old workspace and the entry name its link gets.
struct OldDir {
    path: PathBuf,
    name: String,
}

/// The two parts of `0003_workspace_folders.sql`, around the rows written in between.
fn parts() -> (&'static str, &'static str) {
    MIGRATION_3
        .split_once("-- part: swap")
        .unwrap_or((MIGRATION_3, ""))
}

fn to_workspace_folders(tx: &Transaction<'_>, root: &Path) -> Result<()> {
    let old = old_threads(tx)?;
    let (schema, swap) = parts();
    tx.execute_batch(schema)?;
    if !old.is_empty() {
        std::fs::create_dir_all(root).map_err(|error| io(root, &error))?;
    }
    let mut names: Vec<String> = Vec::new();
    for old in &old {
        let ws = new_folder(tx, root, old, &mut names)?;
        insert_thread_v3(tx, old, &ws.id)?;
        info!(thread = %old.thread.id, workspace = %ws.path.display(), "thread moved to a workspace folder");
    }
    tx.execute_batch(swap)?;
    // The old workspaces were the session's roots: recorded, so the first start removes them (DMN-029).
    let mut stmt =
        tx.prepare("INSERT OR IGNORE INTO thread_roots (thread_id, path) VALUES (?1, ?2)")?;
    for old in &old {
        for dir in &old.dirs {
            stmt.execute(params![old.thread.id, path_str(&dir.path)?])?;
        }
    }
    Ok(())
}

fn io(path: &Path, error: &std::io::Error) -> StoreError {
    StoreError::Migration(format!("{}: {error}", path.display()))
}

fn old_threads(tx: &Transaction<'_>) -> Result<Vec<OldThread>> {
    let threads: Vec<(Thread, Option<String>)> = {
        let mut stmt = tx.prepare(
            "SELECT id, title, session_dir, omp_session_id, approval_mode, model, thinking_level, created_by,
                 created_at, archived_at, created_client
             FROM threads ORDER BY id",
        )?;
        stmt.query_map([], |row| {
            Ok((
                Thread {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    workspace_id: String::new(),
                    session_dir: PathBuf::from(row.get::<_, String>(2)?),
                    omp_session_id: row.get(3)?,
                    approval_mode: row.get(4)?,
                    model: row.get(5)?,
                    thinking_level: row.get(6)?,
                    created_by: Provenance::parse(&row.get::<_, String>(7)?),
                    created_at: row.get(8)?,
                    archived_at: row.get(9)?,
                    tags: Vec::new(),
                },
                row.get(10)?,
            ))
        })?
        .collect::<Result<_, _>>()?
    };
    let mut stmt = tx.prepare(
        "SELECT w.path, w.kind, r.name FROM thread_workspaces tw
         JOIN workspaces w ON w.id = tw.workspace_id
         LEFT JOIN repositories r ON r.id = w.repository_id
         WHERE tw.thread_id = ?1 AND w.removed_at IS NULL
         ORDER BY tw.position",
    )?;
    threads
        .into_iter()
        .map(|(thread, client)| {
            let dirs = stmt
                .query_map([&thread.id], |row| {
                    let path = PathBuf::from(row.get::<_, String>(0)?);
                    let kind: String = row.get(1)?;
                    let repository: Option<String> = row.get(2)?;
                    Ok(old_dir(path, &kind, repository))
                })?
                .collect::<Result<_, _>>()?;
            Ok(OldThread {
                thread,
                client,
                dirs,
            })
        })
        .collect()
}

/// The link name of an old workspace directory: a managed worktree (`worktrees/<repo>/<slug>`) is named after its
/// repository, an adopted directory after itself.
fn old_dir(path: PathBuf, kind: &str, repository: Option<String>) -> OldDir {
    let file_name = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    };
    let name = match (kind, repository) {
        ("managed_worktree", Some(repository)) => Some(repository),
        ("managed_worktree", None) => path.parent().and_then(file_name),
        _ => None,
    }
    .or_else(|| file_name(&path))
    .unwrap_or_else(|| "repository".to_owned());
    OldDir { path, name }
}

/// Creates the thread's owned folder (named after the thread, unique on disk and among names) with its links, and
/// inserts the workspace row.
fn new_folder(
    tx: &Transaction<'_>,
    root: &Path,
    old: &OldThread,
    names: &mut Vec<String>,
) -> Result<Workspace> {
    let title = old.thread.title.trim();
    let title = if title.is_empty() { "workspace" } else { title };
    let name = unique(title, " ", |name| {
        Ok::<_, StoreError>(names.iter().any(|n| n == name))
    })?;
    let slug = unique(&slugify(title), "-", |slug| {
        Ok::<_, StoreError>(std::fs::symlink_metadata(root.join(slug)).is_ok())
    })?;
    let path = root.join(slug);
    std::fs::create_dir(&path).map_err(|error| io(&path, &error))?;
    let path = std::fs::canonicalize(&path).map_err(|error| io(&path, &error))?;
    let mut entries: Vec<String> = Vec::new();
    for dir in &old.dirs {
        if !dir.path.is_dir() {
            warn!(thread = %old.thread.id, dir = %dir.path.display(), "old workspace directory is gone; not linked");
            continue;
        }
        let entry = unique(&dir.name, "-", |entry| {
            Ok::<_, StoreError>(entries.iter().any(|e| e == entry))
        })?;
        let link = path.join(&entry);
        std::os::unix::fs::symlink(&dir.path, &link).map_err(|error| io(&link, &error))?;
        entries.push(entry);
    }
    let ws = Workspace {
        id: new_id(),
        name: name.clone(),
        path,
        owned: true,
        created_by: old.thread.created_by.clone(),
        created_at: old.thread.created_at,
    };
    insert_workspace(tx, &ws, old.client.as_deref())?;
    names.push(name);
    Ok(ws)
}

fn insert_thread_v3(tx: &Transaction<'_>, old: &OldThread, workspace_id: &str) -> Result<()> {
    let thread = &old.thread;
    tx.execute(
        "INSERT INTO threads_v3 (id, title, workspace_id, session_dir, omp_session_id, approval_mode, model,
             thinking_level, created_by, created_client, created_at, archived_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            thread.id,
            thread.title,
            workspace_id,
            path_str(&thread.session_dir)?,
            thread.omp_session_id,
            thread.approval_mode,
            thread.model,
            thread.thinking_level,
            thread.created_by.as_str(),
            old.client,
            thread.created_at,
            thread.archived_at
        ],
    )?;
    Ok(())
}
