//! L2: CRUD on the SQLite store.

use omfp_daemon::store::{
    Provenance, Repository, Store, StoreError, Thread, Workspace, WorkspaceKind, new_id, now_ms,
};
use serde_json::json;
use tempfile::TempDir;

fn open() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let store = Store::open(&dir.path().join("omfp.db")).unwrap();
    (dir, store)
}

fn repository(path: &str) -> Repository {
    Repository {
        id: new_id(),
        name: "app".into(),
        path: path.into(),
        default_base_ref: "main".into(),
        remote_url: Some("git@example.com:app.git".into()),
        created_at: now_ms(),
    }
}

fn workspace(path: &str, repository_id: Option<&str>) -> Workspace {
    Workspace {
        id: new_id(),
        path: path.into(),
        kind: WorkspaceKind::ManagedWorktree,
        repository_id: repository_id.map(str::to_owned),
        branch: Some("omfp/x-1".into()),
        base_ref: Some("main".into()),
        pr_url: None,
        pr_number: None,
        created_by: Provenance::User,
        created_at: now_ms(),
        removed_at: None,
    }
}

fn thread(created_by: Provenance, workspace_ids: Vec<String>) -> Thread {
    let id = new_id();
    Thread {
        session_dir: format!("/data/threads/{id}/sessions").into(),
        id,
        title: "Fix login".into(),
        omp_session_id: None,
        approval_mode: Some("always-ask".into()),
        model: Some("fake/scripted".into()),
        thinking_level: Some("high".into()),
        created_by,
        created_at: now_ms(),
        archived_at: None,
        workspace_ids,
        tags: vec!["auth".into(), "backend".into()],
    }
}

#[test]
fn repositories_round_trip_and_are_listed() {
    let (_dir, store) = open();
    let repo = repository("/src/app");
    store.insert_repository(&repo).unwrap();

    assert_eq!(store.repository(&repo.id).unwrap(), repo);
    assert_eq!(
        store.repository_by_path("/src/app".as_ref()).unwrap(),
        Some(repo.clone())
    );
    assert_eq!(
        store.repository_by_path("/src/other".as_ref()).unwrap(),
        None
    );
    assert_eq!(store.repositories().unwrap(), vec![repo]);
}

// spec: DMN-020
#[test]
fn a_repository_path_is_registered_once() {
    let (_dir, store) = open();
    store.insert_repository(&repository("/src/app")).unwrap();

    let error = store
        .insert_repository(&repository("/src/app"))
        .unwrap_err();
    assert!(matches!(error, StoreError::Conflict(_)), "{error:?}");
}

#[test]
fn unknown_ids_are_not_found() {
    let (_dir, store) = open();
    for error in [
        store.repository("nope").unwrap_err(),
        store.workspace("nope").unwrap_err(),
        store.thread("nope").unwrap_err(),
        store.delete_repository("nope").unwrap_err(),
        store.delete_thread("nope").unwrap_err(),
        store.set_thread_title("nope", "x").unwrap_err(),
    ] {
        assert!(matches!(error, StoreError::NotFound { .. }), "{error:?}");
    }
}

// spec: DMN-020
#[test]
fn unregistering_a_repository_keeps_its_workspaces() {
    let (_dir, store) = open();
    let repo = repository("/src/app");
    store.insert_repository(&repo).unwrap();
    let ws = workspace("/data/worktrees/app/x-1", Some(&repo.id));
    store.insert_workspace(&ws).unwrap();

    store.delete_repository(&repo.id).unwrap();

    assert_eq!(store.repositories().unwrap(), Vec::new());
    assert_eq!(store.workspace(&ws.id).unwrap().repository_id, None);
}

#[test]
fn a_live_workspace_path_is_unique_but_reusable_after_removal() {
    let (_dir, store) = open();
    let first = workspace("/work/a", None);
    store.insert_workspace(&first).unwrap();

    let error = store
        .insert_workspace(&workspace("/work/a", None))
        .unwrap_err();
    assert!(matches!(error, StoreError::Conflict(_)), "{error:?}");

    store.mark_workspace_removed(&first.id, 42).unwrap();
    let second = workspace("/work/a", None);
    store.insert_workspace(&second).unwrap();

    assert_eq!(store.workspace(&first.id).unwrap().removed_at, Some(42));
    assert_eq!(store.workspaces().unwrap(), vec![second.clone()]);
    assert_eq!(
        store.live_workspace_by_path("/work/a".as_ref()).unwrap(),
        Some(second)
    );
}

// spec: DMN-027
#[test]
fn removing_a_workspace_detaches_it_from_threads() {
    let (_dir, store) = open();
    let a = workspace("/work/a", None);
    let b = workspace("/work/b", None);
    store.insert_workspace(&a).unwrap();
    store.insert_workspace(&b).unwrap();
    let t = thread(Provenance::User, vec![a.id.clone(), b.id.clone()]);
    store.insert_thread(&t).unwrap();

    store.mark_workspace_removed(&a.id, now_ms()).unwrap();

    assert_eq!(store.thread(&t.id).unwrap().workspace_ids, vec![b.id]);
    assert_eq!(
        store.workspace_thread_ids(&a.id).unwrap(),
        Vec::<String>::new()
    );
}

// spec: ARC-008, ARC-009
#[test]
fn threads_round_trip_with_ordered_workspaces_tags_and_provenance() {
    let (_dir, store) = open();
    let a = workspace("/work/a", None);
    let b = workspace("/work/b", None);
    store.insert_workspace(&a).unwrap();
    store.insert_workspace(&b).unwrap();
    let parent = thread(Provenance::User, Vec::new());
    store.insert_thread(&parent).unwrap();
    let child = thread(
        Provenance::Thread(parent.id.clone()),
        vec![b.id.clone(), a.id.clone()],
    );
    store.insert_thread(&child).unwrap();

    assert_eq!(store.thread(&child.id).unwrap(), child);
    assert_eq!(store.threads().unwrap(), vec![parent, child.clone()]);
    assert_eq!(store.workspace_thread_ids(&a.id).unwrap(), vec![child.id]);
}

// spec: ARC-008
#[test]
fn thread_workspaces_are_replaced_in_order() {
    let (_dir, store) = open();
    let ids: Vec<String> = ["/w/a", "/w/b", "/w/c"]
        .into_iter()
        .map(|path| {
            let ws = workspace(path, None);
            store.insert_workspace(&ws).unwrap();
            ws.id
        })
        .collect();
    let t = thread(Provenance::User, ids.clone());
    store.insert_thread(&t).unwrap();

    let reordered = vec![ids[2].clone(), ids[0].clone()];
    store.set_thread_workspaces(&t.id, &reordered).unwrap();

    assert_eq!(store.thread(&t.id).unwrap().workspace_ids, reordered);
}

#[test]
fn thread_fields_are_updated() {
    let (_dir, store) = open();
    let t = thread(Provenance::User, Vec::new());
    store.insert_thread(&t).unwrap();

    store.set_thread_title(&t.id, "Renamed").unwrap();
    store
        .set_thread_tags(&t.id, &["zeta".into(), "alpha".into(), "zeta".into()])
        .unwrap();
    store.set_thread_archived_at(&t.id, Some(7)).unwrap();
    store
        .set_thread_omp_session_id(&t.id, Some("sess-1"))
        .unwrap();
    store.set_thread_model(&t.id, None).unwrap();
    store.set_thread_thinking_level(&t.id, Some("low")).unwrap();

    let updated = store.thread(&t.id).unwrap();
    assert_eq!(
        updated,
        Thread {
            title: "Renamed".into(),
            tags: vec!["alpha".into(), "zeta".into()],
            archived_at: Some(7),
            omp_session_id: Some("sess-1".into()),
            model: None,
            thinking_level: Some("low".into()),
            ..t
        }
    );
}

#[test]
fn settings_hold_json_values() {
    let (_dir, store) = open();
    assert_eq!(store.setting("idle_timeout").unwrap(), None);

    store
        .set_setting("idle_timeout", &json!({"minutes": 15}))
        .unwrap();
    store
        .set_setting("idle_timeout", &json!({"minutes": 5}))
        .unwrap();

    assert_eq!(
        store.setting("idle_timeout").unwrap(),
        Some(json!({"minutes": 5}))
    );
}
