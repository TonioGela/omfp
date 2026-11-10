//! L2: the SQLite store on a real file — migrations, pragmas, constraints.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use omfp_daemon::store::{
    Provenance, Store, StoreError, Thread, TitleSource, Workspace, new_id, now_ms,
};
use rusqlite::{Connection, params};
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

fn count(db: &Path, table: &str) -> i64 {
    raw(db)
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

/// The sorted entry names of a directory.
fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn open(dir: &TempDir) -> Store {
    Store::open(&dir.path().join("omfp.db"), &dir.path().join("workspaces")).unwrap()
}

fn workspace(name: &str) -> Workspace {
    Workspace {
        id: new_id(),
        name: name.into(),
        path: PathBuf::from("/work").join(name),
        owned: true,
        created_by: Provenance::User,
        created_at: now_ms(),
    }
}

fn thread(workspace_id: &str, tags: Vec<String>) -> Thread {
    Thread {
        id: new_id(),
        title: "t".into(),
        title_source: TitleSource::Placeholder,
        workspace_id: workspace_id.into(),
        session_dir: "/data/threads/x/sessions".into(),
        omp_session_id: None,
        approval_mode: None,
        model: None,
        thinking_level: None,
        created_by: Provenance::User,
        created_at: now_ms(),
        archived_at: None,
        tags,
    }
}

// spec: DMN-010, DMN-012
#[test]
fn opening_an_empty_file_applies_every_migration() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("omfp.db");
    drop(open(&dir));

    assert_eq!(user_version(&db), 4);
    // Only metadata lives in the database: no transcript tables, no repositories of workspaces (DMN-022).
    assert_eq!(
        tables(&db),
        [
            "repositories",
            "settings",
            "thread_roots",
            "thread_tags",
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
    let ws = workspace("a");
    let t = thread(&ws.id, vec!["x".into()]);
    {
        let store = open(&dir);
        store.insert_workspace(&ws).unwrap();
        store.insert_thread(&t).unwrap();
    }
    let store = open(&dir);
    assert_eq!(user_version(&db), 4);
    assert_eq!(store.workspaces().unwrap(), vec![ws]);
    assert_eq!(store.threads().unwrap(), vec![t]);
}

// spec: DMN-010, DMN-073
#[test]
fn migration_4_counts_existing_titles_as_set() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("omfp.db");
    let ws = workspace("a");
    let t = thread(&ws.id, Vec::new());
    {
        let store = open(&dir);
        store.insert_workspace(&ws).unwrap();
        store.insert_thread(&t).unwrap();
    }
    // Back to a version-3 database: no `title_source` column.
    let conn = raw(&db);
    conn.execute_batch("ALTER TABLE threads DROP COLUMN title_source; PRAGMA user_version = 3;")
        .unwrap();
    drop(conn);

    let store = open(&dir);
    assert_eq!(user_version(&db), 4);
    assert_eq!(
        store.threads().unwrap(),
        vec![Thread {
            title_source: TitleSource::Set,
            ..t
        }]
    );
}

// spec: DMN-010
#[test]
fn a_database_from_a_newer_daemon_is_refused() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("omfp.db");
    raw(&db).pragma_update(None, "user_version", 99).unwrap();

    let error = Store::open(&db, &dir.path().join("workspaces")).unwrap_err();
    assert!(
        matches!(
            error,
            StoreError::UnknownSchemaVersion {
                found: 99,
                supported: 4
            }
        ),
        "{error:?}"
    );
}

// spec: DMN-011
#[test]
fn the_database_uses_write_ahead_logging() {
    let dir = TempDir::new().unwrap();
    let _store = open(&dir);

    let mode: String = raw(&dir.path().join("omfp.db"))
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
}

// spec: DMN-011
#[test]
fn foreign_keys_are_enforced() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir);

    let error = store
        .insert_thread(&thread("missing-workspace", Vec::new()))
        .unwrap_err();
    assert!(matches!(error, StoreError::Sqlite(_)), "{error:?}");
    assert_eq!(store.threads().unwrap(), Vec::new());
}

// spec: DMN-011
#[test]
fn deleting_a_thread_cascades_to_its_roots_and_tags() {
    let dir = TempDir::new().unwrap();
    let db = dir.path().join("omfp.db");
    let store = open(&dir);
    let ws = workspace("a");
    store.insert_workspace(&ws).unwrap();
    let t = thread(&ws.id, vec!["feature".into()]);
    store.insert_thread(&t).unwrap();
    store
        .set_thread_roots(&t.id, &[PathBuf::from("/work/a/app")])
        .unwrap();

    store.delete_thread(&t.id).unwrap();

    assert_eq!(
        store.workspace_thread_ids(&ws.id).unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(count(&db, "thread_tags"), 0);
    assert_eq!(count(&db, "thread_roots"), 0);
    // The workspace itself is untouched.
    assert_eq!(store.workspace(&ws.id).unwrap(), ws);
}

/// A database at `user_version = 2` (the old model) with its directories on disk.
struct OldModel {
    db: PathBuf,
    root: PathBuf,
    /// Created by the user from the old web UI (client `omfp-ui`); managed worktree, adopted dirs, a removed and a
    /// missing workspace.
    first: Thread,
    /// Created by `first`'s agent, archived, no workspace.
    second: Thread,
    worktree: PathBuf,
    notes: PathBuf,
    removed: PathBuf,
    gone: PathBuf,
    other_shop: PathBuf,
}

fn old_thread(base: &Path, title: &str, created_by: Provenance) -> Thread {
    let id = new_id();
    Thread {
        session_dir: base.join("data/threads").join(&id).join("sessions"),
        id,
        title: title.into(),
        title_source: TitleSource::Set,
        workspace_id: String::new(),
        omp_session_id: None,
        approval_mode: None,
        model: None,
        thinking_level: None,
        created_by,
        created_at: now_ms(),
        archived_at: None,
        tags: Vec::new(),
    }
}

fn insert_old_thread(conn: &Connection, t: &Thread, client: Option<&str>) {
    conn.execute(
        "INSERT INTO threads (id, title, session_dir, omp_session_id, approval_mode, model, thinking_level,
             created_by, created_at, archived_at, created_client)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            t.id,
            t.title,
            t.session_dir.to_str().unwrap(),
            t.omp_session_id,
            t.approval_mode,
            t.model,
            t.thinking_level,
            t.created_by.as_str(),
            t.created_at,
            t.archived_at,
            client
        ],
    )
    .unwrap();
    for tag in &t.tags {
        conn.execute(
            "INSERT INTO thread_tags (thread_id, tag) VALUES (?1, ?2)",
            params![t.id, tag],
        )
        .unwrap();
    }
}

/// Inserts an old workspace attached to thread `old.0` at position `old.1`.
fn insert_old_workspace(
    conn: &Connection,
    old: (&str, i64),
    path: &Path,
    kind: &str,
    repo: Option<&str>,
    removed: bool,
) {
    let id = new_id();
    conn.execute(
        "INSERT INTO workspaces (id, path, kind, repository_id, branch, base_ref, created_by, created_at, removed_at,
             created_client)
         VALUES (?1, ?2, ?3, ?4, 'omfp/fix-login-1', 'main', 'user', ?5, ?6, 'omfp-ui')",
        params![id, path.to_str().unwrap(), kind, repo, now_ms(), removed.then_some(7_i64)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO thread_workspaces (thread_id, workspace_id, position) VALUES (?1, ?2, ?3)",
        params![old.0, id, old.1],
    )
    .unwrap();
}

/// Builds the old-model database in `dir`: migrations 1 and 2 only, then old rows and directories.
fn old_model(dir: &TempDir) -> OldModel {
    let base = dir.path().canonicalize().unwrap();
    let worktree = base.join("data/worktrees/shop/fix-login-1");
    let notes = base.join("src/notes");
    let removed = base.join("src/old");
    let other_shop = base.join("other/shop");
    for path in [&worktree, &notes, &removed, &other_shop] {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("README"), path.to_str().unwrap()).unwrap();
    }
    let mut first = old_thread(&base, "Fix login", Provenance::User);
    first.omp_session_id = Some("sess-1".into());
    first.model = Some("fake/scripted".into());
    first.thinking_level = Some("high".into());
    first.approval_mode = Some("always-ask".into());
    first.tags = vec!["auth".into(), "backend".into()];
    let mut second = old_thread(&base, "Fix login", Provenance::Thread(first.id.clone()));
    second.archived_at = Some(42);

    let db = base.join("omfp.db");
    let conn = raw(&db);
    conn.execute_batch(include_str!("../src/store/0001_initial.sql"))
        .unwrap();
    conn.execute_batch(include_str!("../src/store/0002_created_client.sql"))
        .unwrap();
    conn.pragma_update(None, "user_version", 2).unwrap();
    let repo = new_id();
    conn.execute(
        "INSERT INTO repositories (id, name, path, default_base_ref, remote_url, created_at)
         VALUES (?1, 'shop', ?2, 'main', NULL, 1)",
        params![repo, base.join("src/shop").to_str().unwrap()],
    )
    .unwrap();
    insert_old_thread(&conn, &first, Some("omfp-ui"));
    insert_old_thread(&conn, &second, None);
    let gone = base.join("src/gone");
    let old = |position| (first.id.as_str(), position);
    insert_old_workspace(
        &conn,
        old(0),
        &worktree,
        "managed_worktree",
        Some(&repo),
        false,
    );
    insert_old_workspace(&conn, old(1), &notes, "adopted", None, false);
    insert_old_workspace(&conn, old(2), &removed, "adopted", None, true);
    insert_old_workspace(&conn, old(3), &gone, "adopted", None, false);
    insert_old_workspace(&conn, old(4), &other_shop, "adopted", None, false);
    OldModel {
        db,
        root: base.join("workspaces"),
        first,
        second,
        worktree,
        notes,
        removed,
        gone,
        other_shop,
    }
}

/// The new workspaces of the migrated threads and the threads themselves.
fn assert_migrated_records(store: &Store, old: &OldModel) -> (Workspace, Workspace) {
    let workspaces = store.workspaces().unwrap();
    let [one, two] = <[Workspace; 2]>::try_from(workspaces).unwrap();
    assert_eq!(
        one,
        Workspace {
            id: one.id.clone(),
            name: "Fix login".into(),
            path: old.root.join("fix-login"),
            owned: true,
            created_by: Provenance::User,
            created_at: old.first.created_at,
        }
    );
    assert_eq!(
        two,
        Workspace {
            id: two.id.clone(),
            name: "Fix login 2".into(),
            path: old.root.join("fix-login-2"),
            owned: true,
            created_by: Provenance::Thread(old.first.id.clone()),
            created_at: old.second.created_at,
        }
    );
    let first = Thread {
        workspace_id: one.id.clone(),
        ..old.first.clone()
    };
    let second = Thread {
        workspace_id: two.id.clone(),
        ..old.second.clone()
    };
    assert_eq!(store.threads().unwrap(), vec![first, second]);
    assert_eq!(
        store.created_clients().unwrap(),
        HashMap::from([
            (old.first.id.clone(), "omfp-ui".to_owned()),
            (one.id.clone(), "omfp-ui".to_owned()),
        ])
    );
    let mut roots = vec![
        old.worktree.clone(),
        old.notes.clone(),
        old.gone.clone(),
        old.other_shop.clone(),
    ];
    roots.sort();
    assert_eq!(store.thread_roots(&old.first.id).unwrap(), roots);
    assert_eq!(
        store.thread_roots(&old.second.id).unwrap(),
        Vec::<PathBuf>::new()
    );
    (one, two)
}

fn assert_migrated_folders(old: &OldModel, one: &Workspace, two: &Workspace) {
    assert_eq!(entries(&old.root), ["fix-login", "fix-login-2"]);
    assert_eq!(entries(&one.path), ["notes", "shop", "shop-2"]);
    for (entry, target) in [
        ("shop", &old.worktree),
        ("notes", &old.notes),
        ("shop-2", &old.other_shop),
    ] {
        assert_eq!(std::fs::read_link(one.path.join(entry)).unwrap(), *target);
    }
    assert_eq!(entries(&two.path), Vec::<String>::new());
    // The old directories stay where they were, untouched.
    for path in [&old.worktree, &old.notes, &old.removed, &old.other_shop] {
        assert_eq!(entries(path), ["README"]);
        assert_eq!(
            std::fs::read_to_string(path.join("README")).unwrap(),
            path.to_str().unwrap()
        );
    }
    assert!(!old.gone.exists());
}

// spec: DMN-010
#[test]
fn migration_3_moves_every_thread_into_its_own_workspace_folder() {
    let dir = TempDir::new().unwrap();
    let old = old_model(&dir);

    let store = Store::open(&old.db, &old.root).unwrap();

    assert_eq!(user_version(&old.db), 4);
    assert_eq!(
        tables(&old.db),
        [
            "repositories",
            "settings",
            "thread_roots",
            "thread_tags",
            "threads",
            "workspaces"
        ]
    );
    assert_eq!(store.repositories().unwrap().len(), 1);
    let (one, two) = assert_migrated_records(&store, &old);
    assert_migrated_folders(&old, &one, &two);

    // Reopening is a no-op: same records, no new folders.
    drop(store);
    let store = Store::open(&old.db, &old.root).unwrap();
    assert_eq!(user_version(&old.db), 4);
    assert_eq!(
        assert_migrated_records(&store, &old),
        (one.clone(), two.clone())
    );
    assert_migrated_folders(&old, &one, &two);
}
