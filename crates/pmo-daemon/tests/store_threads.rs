//! L2: the thread service on a real SQLite file and data directory.

use std::path::PathBuf;
use std::sync::Arc;

use pmo_daemon::paths::DataDir;
use pmo_daemon::store::{Provenance, Store, StoreError, Workspace, WorkspaceKind, new_id, now_ms};
use pmo_daemon::threads::{Attachment, DirChange, NewThread, ThreadError, Threads};
use tempfile::TempDir;

struct Env {
    _dir: TempDir,
    data: DataDir,
    store: Arc<Store>,
    threads: Threads,
}

fn env() -> Env {
    let dir = TempDir::new().unwrap();
    let data = DataDir::new(dir.path().canonicalize().unwrap().join("data"));
    std::fs::create_dir_all(data.root()).unwrap();
    let store = Arc::new(Store::open(&data.db()).unwrap());
    let threads = Threads::new(store.clone(), data.clone());
    Env {
        _dir: dir,
        data,
        store,
        threads,
    }
}

impl Env {
    /// Records an adopted workspace at a made-up path; the thread service never touches workspace directories.
    fn workspace(&self, name: &str) -> Workspace {
        let ws = Workspace {
            id: new_id(),
            path: PathBuf::from("/work").join(name),
            kind: WorkspaceKind::Adopted,
            repository_id: None,
            branch: None,
            base_ref: None,
            pr_url: None,
            pr_number: None,
            created_by: Provenance::User,
            created_at: now_ms(),
            removed_at: None,
        };
        self.store.insert_workspace(&ws).unwrap();
        ws
    }
}

// spec: DMN-028, ARC-020
#[test]
fn a_new_thread_gets_its_own_home_and_session_directories() {
    let env = env();

    let thread = env.threads.create(NewThread::new("Fix login")).unwrap();

    assert!(env.data.thread_home(&thread.id).is_dir());
    assert_eq!(thread.session_dir, env.data.thread_sessions(&thread.id));
    assert!(thread.session_dir.is_dir());
    assert_eq!(thread.title, "Fix login");
    assert_eq!(thread.omp_session_id, None);
    assert_eq!(env.threads.list().unwrap(), vec![thread]);
}

// spec: ARC-008, ARC-009
#[test]
fn a_thread_records_workspaces_tags_settings_and_provenance() {
    let env = env();
    let a = env.workspace("a");
    let b = env.workspace("b");
    let parent = env.threads.create(NewThread::new("parent")).unwrap();

    let thread = env
        .threads
        .create(NewThread {
            workspace_ids: vec![b.id.clone(), a.id.clone()],
            tags: vec!["login".into(), "auth".into()],
            model: Some("fake/scripted".into()),
            thinking_level: Some("high".into()),
            approval_mode: Some("always-ask".into()),
            created_by: Provenance::Thread(parent.id.clone()),
            ..NewThread::new("child")
        })
        .unwrap();

    assert_eq!(thread.workspace_ids, vec![b.id, a.id]);
    assert_eq!(thread.tags, vec!["auth", "login"]);
    assert_eq!(thread.model.as_deref(), Some("fake/scripted"));
    assert_eq!(thread.thinking_level.as_deref(), Some("high"));
    assert_eq!(thread.approval_mode.as_deref(), Some("always-ask"));
    assert_eq!(thread.created_by, Provenance::Thread(parent.id));
    assert_eq!(env.threads.get(&thread.id).unwrap(), thread);
    assert_eq!(
        env.threads.workspace_paths(&thread.id).unwrap(),
        vec![PathBuf::from("/work/b"), PathBuf::from("/work/a")]
    );
}

// spec: ARC-009
#[test]
fn creation_is_refused_for_unknown_creators_or_workspaces() {
    let env = env();
    let ws = env.workspace("a");

    let errors = [
        env.threads
            .create(NewThread {
                created_by: Provenance::Thread("01JUNKNOWN0000000000000000".into()),
                ..NewThread::new("x")
            })
            .unwrap_err(),
        env.threads
            .create(NewThread {
                workspace_ids: vec!["missing".into()],
                ..NewThread::new("x")
            })
            .unwrap_err(),
    ];
    for error in errors {
        assert!(
            matches!(error, ThreadError::Store(StoreError::NotFound { .. })),
            "{error:?}"
        );
    }
    let duplicate = env
        .threads
        .create(NewThread {
            workspace_ids: vec![ws.id.clone(), ws.id.clone()],
            ..NewThread::new("x")
        })
        .unwrap_err();
    assert!(
        matches!(duplicate, ThreadError::DuplicateWorkspace(_)),
        "{duplicate:?}"
    );
    assert_eq!(env.threads.list().unwrap(), Vec::new());
}

// spec: ARC-008, DMN-029
#[test]
fn attaching_and_detaching_keeps_the_order_and_reports_directory_changes() {
    let env = env();
    let [a, b, c] = ["a", "b", "c"].map(|name| env.workspace(name));
    let ids =
        |list: &[&Workspace]| -> Vec<String> { list.iter().map(|ws| ws.id.clone()).collect() };
    let thread = env.threads.create(NewThread::new("t")).unwrap();

    let Attachment { change, .. } = env.threads.attach(&thread.id, &a.id, None).unwrap();
    assert_eq!(change, Some(DirChange::Added(a.path.clone())));
    env.threads.attach(&thread.id, &b.id, None).unwrap();
    let first = env.threads.attach(&thread.id, &c.id, Some(0)).unwrap();
    assert_eq!(first.thread.workspace_ids, ids(&[&c, &a, &b]));
    assert_eq!(first.change, Some(DirChange::Added(c.path.clone())));

    // Moving an attached workspace only reorders.
    let moved = env.threads.attach(&thread.id, &c.id, Some(9)).unwrap();
    assert_eq!(moved.thread.workspace_ids, ids(&[&a, &b, &c]));
    assert_eq!(moved.change, None);
    let again = env.threads.attach(&thread.id, &a.id, None).unwrap();
    assert_eq!(again.thread.workspace_ids, ids(&[&a, &b, &c]));
    assert_eq!(again.change, None);

    let detached = env.threads.detach(&thread.id, &a.id).unwrap();
    assert_eq!(detached.thread.workspace_ids, ids(&[&b, &c]));
    assert_eq!(detached.change, Some(DirChange::Removed(a.path.clone())));
    let noop = env.threads.detach(&thread.id, &a.id).unwrap();
    assert_eq!(noop.change, None);
    assert_eq!(
        env.threads.get(&thread.id).unwrap().workspace_ids,
        ids(&[&b, &c])
    );
}

// spec: DMN-027
#[test]
fn a_workspace_can_be_shared_by_threads() {
    let env = env();
    let ws = env.workspace("shared");
    let one = env
        .threads
        .create(NewThread {
            workspace_ids: vec![ws.id.clone()],
            ..NewThread::new("one")
        })
        .unwrap();
    let two = env.threads.create(NewThread::new("two")).unwrap();

    let attached = env.threads.attach(&two.id, &ws.id, None).unwrap();

    assert_eq!(attached.change, Some(DirChange::Added(ws.path.clone())));
    assert_eq!(
        env.store.workspace_thread_ids(&ws.id).unwrap(),
        vec![one.id, two.id]
    );
}

#[test]
fn title_tags_settings_session_and_archive_are_updated() {
    let env = env();
    let thread = env.threads.create(NewThread::new("t")).unwrap();

    env.threads.rename(&thread.id, "Renamed").unwrap();
    env.threads
        .set_tags(&thread.id, &["b".into(), "a".into()])
        .unwrap();
    env.threads.set_model(&thread.id, Some("p/m")).unwrap();
    env.threads
        .set_thinking_level(&thread.id, Some("low"))
        .unwrap();
    env.threads
        .set_omp_session_id(&thread.id, "sess-1")
        .unwrap();
    let archived = env.threads.archive(&thread.id).unwrap();

    assert_eq!(archived.title, "Renamed");
    assert_eq!(archived.tags, vec!["a", "b"]);
    assert_eq!(archived.model.as_deref(), Some("p/m"));
    assert_eq!(archived.thinking_level.as_deref(), Some("low"));
    assert_eq!(archived.omp_session_id.as_deref(), Some("sess-1"));
    assert!(archived.archived_at.is_some());
}

#[test]
fn deleting_a_thread_removes_its_directory_but_never_its_workspaces() {
    let env = env();
    let ws = env.workspace("a");
    let thread = env
        .threads
        .create(NewThread {
            workspace_ids: vec![ws.id.clone()],
            ..NewThread::new("t")
        })
        .unwrap();

    env.threads.delete(&thread.id).unwrap();

    assert!(!env.data.thread(&thread.id).exists());
    assert_eq!(env.threads.list().unwrap(), Vec::new());
    assert_eq!(env.store.workspaces().unwrap(), vec![ws]);
    assert!(matches!(
        env.threads.delete(&thread.id),
        Err(ThreadError::Store(StoreError::NotFound { .. }))
    ));
}
