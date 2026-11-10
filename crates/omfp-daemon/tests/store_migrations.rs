//! L2: the SQLite store on a real file — migrations, pragmas, constraints.

use std::path::Path;

use omfp_daemon::store::{
    Provenance, Store, StoreError, Thread, Workspace, WorkspaceKind, new_id, now_ms,
};
use rusqlite::Connection;
use tempfile::TempDir;

fn raw(path: &Path) -> Connection {
    Connection::open(path).unwrap()
}

fn user_version(path: &Path) -> i64 {
    raw(path)
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap()
}

fn tables(path: &Path) -> Vec<String> {
    let conn = raw(path);
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap();
    stmt.query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn workspace(path: &str) -> Workspace {
    Workspace {
        id: new_id(),
        path: path.into(),
        kind: WorkspaceKind::Adopted,
        repository_id: None,
        branch: None,
        base_ref: None,
        pr_url: None,
        pr_number: None,
        created_by: Provenance::User,
        created_at: now_ms(),
        removed_at: None,
    }
}

fn thread(workspace_ids: Vec<String>, tags: Vec<String>) -> Thread {
    Thread {
        id: new_id(),
        title: "t".into(),
        session_dir: "/data/threads/x/sessions".into(),
        omp_session_id: None,
        approval_mode: None,
        model: None,
        thinking_level: None,
        created_by: Provenance::User,
        created_at: now_ms(),
        archived_at: None,
        workspace_ids,
        tags,
    }
}

// spec: DMN-010, DMN-012
#[test]
fn opening_an_empty_file_applies_every_migration() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("omfp.db");
    drop(Store::open(&db).unwrap());

    assert_eq!(user_version(&db), 2);
    // Only metadata lives in the database: no transcript tables.
    assert_eq!(
        tables(&db),
        [
            "repositories",
            "settings",
            "thread_tags",
            "thread_workspaces",
            "threads",
            "workspaces"
        ]
    );
}

// spec: DMN-010
#[test]
fn reopening_is_idempotent_and_keeps_data() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("omfp.db");
    let ws = workspace("/work/a");
    {
        let store = Store::open(&db).unwrap();
        store.insert_workspace(&ws).unwrap();
    }
    let store = Store::open(&db).unwrap();
    assert_eq!(user_version(&db), 2);
    assert_eq!(store.workspace(&ws.id).unwrap(), ws);
}

// spec: DMN-010
#[test]
fn a_database_from_a_newer_daemon_is_refused() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("omfp.db");
    raw(&db).pragma_update(None, "user_version", 99).unwrap();

    let error = Store::open(&db).unwrap_err();
    assert!(
        matches!(
            error,
            StoreError::UnknownSchemaVersion {
                found: 99,
                supported: 2
            }
        ),
        "{error:?}"
    );
}

// spec: DMN-011
#[test]
fn the_database_uses_write_ahead_logging() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("omfp.db");
    let _store = Store::open(&db).unwrap();

    let mode: String = raw(&db)
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

// spec: DMN-011
#[test]
fn foreign_keys_are_enforced() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(&dir.path().join("omfp.db")).unwrap();
    let t = thread(Vec::new(), Vec::new());
    store.insert_thread(&t).unwrap();

    let error = store
        .set_thread_workspaces(&t.id, &["missing-workspace".to_owned()])
        .unwrap_err();
    assert!(matches!(error, StoreError::Sqlite(_)), "{error:?}");
    assert_eq!(
        store.thread(&t.id).unwrap().workspace_ids,
        Vec::<String>::new()
    );
}

// spec: DMN-011
#[test]
fn deleting_a_thread_cascades_to_its_links_and_tags() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("omfp.db");
    let store = Store::open(&db).unwrap();
    let ws = workspace("/work/a");
    store.insert_workspace(&ws).unwrap();
    let t = thread(vec![ws.id.clone()], vec!["feature".into()]);
    store.insert_thread(&t).unwrap();

    store.delete_thread(&t.id).unwrap();

    assert_eq!(
        store.workspace_thread_ids(&ws.id).unwrap(),
        Vec::<String>::new()
    );
    let tags: i64 = raw(&db)
        .query_row("SELECT count(*) FROM thread_tags", [], |row| row.get(0))
        .unwrap();
    assert_eq!(tags, 0);
    // The workspace itself is untouched.
    assert_eq!(store.workspace(&ws.id).unwrap(), ws);
}
