//! L2: CRUD on the SQLite store.

use std::collections::HashMap;
use std::path::PathBuf;

use omfp_daemon::store::{
    Provenance, SeenRepository, Store, StoreError, Thread, TitleSource, Workspace, new_id, now_ms,
};
use serde_json::json;
use tempfile::TempDir;

fn open() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let store = Store::open(&dir.path().join("omfp.db"), &dir.path().join("workspaces")).unwrap();
    (dir, store)
}

fn workspace(name: &str, path: &str) -> Workspace {
    Workspace {
        id: new_id(),
        name: name.into(),
        path: path.into(),
        created_by: Provenance::User,
        created_at: now_ms(),
    }
}

fn thread(created_by: Provenance, workspace_id: &str) -> Thread {
    let id = new_id();
    Thread {
        session_dir: format!("/data/threads/{id}/sessions").into(),
        id,
        title: "Fix login".into(),
        title_source: TitleSource::Set,
        workspace_id: workspace_id.into(),
        omp_session_id: None,
        approval_mode: Some("always-ask".into()),
        model: Some("fake/scripted".into()),
        thinking_level: Some("high".into()),
        created_by,
        created_at: now_ms(),
        archived_at: None,
    }
}

/// A store holding one workspace.
fn with_workspace() -> (TempDir, Store, Workspace) {
    let (dir, store) = open();
    let ws = workspace("shop", "/work/shop");
    store.insert_workspace(&ws).unwrap();
    (dir, store, ws)
}

fn assert_conflict<T: std::fmt::Debug>(result: Result<T, StoreError>) {
    let error = result.unwrap_err();
    assert!(matches!(error, StoreError::Conflict(_)), "{error:?}");
}

// spec: DMN-021
#[test]
fn seen_repositories_keep_their_latest_sighting_most_recent_first() {
    let (_dir, store) = open();
    let seen = |path: &str, at: i64| SeenRepository {
        path: path.into(),
        last_seen_at: at,
    };
    store
        .record_seen_repositories(&["/src/app".into(), "/src/api".into()], 10)
        .unwrap();
    store
        .record_seen_repositories(&["/src/lib".into()], 20)
        .unwrap();
    store
        .record_seen_repositories(&["/src/app".into()], 30)
        .unwrap();
    // An older sighting never moves a repository back.
    store
        .record_seen_repositories(&["/src/lib".into()], 5)
        .unwrap();
    store.record_seen_repositories(&[], 40).unwrap();

    assert_eq!(
        store.seen_repositories().unwrap(),
        vec![
            seen("/src/app", 30),
            seen("/src/lib", 20),
            seen("/src/api", 10)
        ]
    );
}

#[test]
fn unknown_ids_are_not_found() {
    let (_dir, store) = open();
    for error in [
        store.workspace("nope").unwrap_err(),
        store.thread("nope").unwrap_err(),
        store.rename_workspace("nope", "x").unwrap_err(),
        store.delete_workspace("nope").unwrap_err(),
        store.delete_thread("nope").unwrap_err(),
        store
            .set_thread_title("nope", "x", TitleSource::Set)
            .unwrap_err(),
        store.set_thread_roots("nope", &[]).unwrap_err(),
        store.set_created_client("nope", "cli").unwrap_err(),
    ] {
        assert!(matches!(error, StoreError::NotFound { .. }), "{error:?}");
    }
}

// spec: DMN-020
#[test]
fn workspaces_round_trip_by_id_path_and_name() {
    let (_dir, store, shop) = with_workspace();
    let made = Workspace {
        created_by: Provenance::Thread("01JAGENT".into()),
        ..workspace("Fix login", "/omfp/workspaces/fix-login")
    };
    store.insert_workspace(&made).unwrap();

    assert_eq!(store.workspace(&made.id).unwrap(), made);
    assert_eq!(
        store.workspace_by_path("/work/shop".as_ref()).unwrap(),
        Some(shop.clone())
    );
    assert_eq!(
        store.workspace_by_path("/work/none".as_ref()).unwrap(),
        None
    );
    assert!(store.workspace_name_taken("Fix login").unwrap());
    assert!(!store.workspace_name_taken("fix login").unwrap());
    assert_eq!(store.workspaces().unwrap(), vec![shop, made]);
}

// spec: DMN-020
#[test]
fn workspace_names_and_paths_are_unique() {
    let (_dir, store, ws) = with_workspace();

    assert_conflict(store.insert_workspace(&workspace("shop", "/work/other")));
    assert_conflict(store.insert_workspace(&workspace("other", "/work/shop")));
    assert_eq!(store.workspaces().unwrap(), vec![ws]);
}

// spec: DMN-020
#[test]
fn renaming_a_workspace_keeps_names_unique() {
    let (_dir, store, ws) = with_workspace();
    let other = workspace("other", "/work/other");
    store.insert_workspace(&other).unwrap();

    assert_conflict(store.rename_workspace(&ws.id, "other"));
    store.rename_workspace(&ws.id, "Shop v2").unwrap();

    assert_eq!(store.workspace(&ws.id).unwrap().name, "Shop v2");
    assert!(!store.workspace_name_taken("shop").unwrap());
    assert_eq!(store.workspace(&other.id).unwrap(), other);
}

// spec: DMN-028
#[test]
fn a_workspace_with_threads_is_not_deleted() {
    let (_dir, store, ws) = with_workspace();
    let t = thread(Provenance::User, &ws.id);
    store.insert_thread(&t).unwrap();
    store.set_thread_archived_at(&t.id, Some(7)).unwrap();

    // Archived threads still live in the workspace.
    assert_conflict(store.delete_workspace(&ws.id));
    assert_eq!(store.workspaces().unwrap(), vec![ws.clone()]);

    store.delete_thread(&t.id).unwrap();
    store.delete_workspace(&ws.id).unwrap();
    assert_eq!(store.workspaces().unwrap(), Vec::new());
}

// spec: ARC-008, ARC-009, DMN-027
#[test]
fn threads_round_trip_with_their_workspace_and_provenance() {
    let (_dir, store, ws) = with_workspace();
    let other = workspace("other", "/work/other");
    store.insert_workspace(&other).unwrap();
    let parent = thread(Provenance::User, &ws.id);
    store.insert_thread(&parent).unwrap();
    let child = thread(Provenance::Thread(parent.id.clone()), &ws.id);
    store.insert_thread(&child).unwrap();

    assert_eq!(store.thread(&child.id).unwrap(), child);
    assert_eq!(
        store.threads().unwrap(),
        vec![parent.clone(), child.clone()]
    );
    assert_eq!(
        store.workspace_thread_ids(&ws.id).unwrap(),
        vec![parent.id, child.id]
    );
    assert_eq!(
        store.workspace_thread_ids(&other.id).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn thread_fields_are_updated() {
    let (_dir, store, ws) = with_workspace();
    let t = thread(Provenance::User, &ws.id);
    store.insert_thread(&t).unwrap();

    store
        .set_thread_title(&t.id, "Renamed", TitleSource::Generated)
        .unwrap();
    store.set_thread_approval_mode(&t.id, "yolo").unwrap();
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
            title_source: TitleSource::Generated,
            approval_mode: Some("yolo".into()),
            archived_at: Some(7),
            omp_session_id: Some("sess-1".into()),
            model: None,
            thinking_level: Some("low".into()),
            ..t
        }
    );
}

// spec: DMN-073
#[test]
fn only_a_placeholder_title_is_replaced() {
    let (_dir, store, ws) = with_workspace();
    let t = Thread {
        title: String::new(),
        title_source: TitleSource::Placeholder,
        ..thread(Provenance::User, &ws.id)
    };
    store.insert_thread(&t).unwrap();

    assert!(
        store
            .replace_placeholder_title(&t.id, "Fix the login…", TitleSource::Placeholder)
            .unwrap()
    );
    store
        .set_thread_title(&t.id, "Mine", TitleSource::Set)
        .unwrap();
    assert!(
        !store
            .replace_placeholder_title(&t.id, "Generated", TitleSource::Generated)
            .unwrap()
    );
    let stored = store.thread(&t.id).unwrap();
    assert_eq!(
        (stored.title.as_str(), stored.title_source),
        ("Mine", TitleSource::Set)
    );
}

// spec: DMN-029
#[test]
fn thread_roots_are_replaced_and_listed_sorted() {
    let (_dir, store, ws) = with_workspace();
    let t = thread(Provenance::User, &ws.id);
    store.insert_thread(&t).unwrap();
    assert_eq!(store.thread_roots(&t.id).unwrap(), Vec::<PathBuf>::new());

    let [api, web] = ["/work/shop/api", "/work/shop/web"].map(PathBuf::from);
    store
        .set_thread_roots(&t.id, &[web.clone(), api.clone(), web.clone()])
        .unwrap();
    assert_eq!(store.thread_roots(&t.id).unwrap(), vec![api, web.clone()]);

    store
        .set_thread_roots(&t.id, std::slice::from_ref(&web))
        .unwrap();
    assert_eq!(store.thread_roots(&t.id).unwrap(), vec![web]);
    store.set_thread_roots(&t.id, &[]).unwrap();
    assert_eq!(store.thread_roots(&t.id).unwrap(), Vec::<PathBuf>::new());
}

// spec: ARC-009, PRT-033
#[test]
fn the_creating_client_is_recorded_for_threads_and_workspaces() {
    let (_dir, store, ws) = with_workspace();
    let t = thread(Provenance::User, &ws.id);
    store.insert_thread(&t).unwrap();
    let silent = thread(Provenance::User, &ws.id);
    store.insert_thread(&silent).unwrap();
    assert_eq!(store.created_clients().unwrap(), HashMap::new());

    store.set_created_client(&t.id, "omfp/web").unwrap();
    store.set_created_client(&ws.id, "omfp-cli").unwrap();

    assert_eq!(
        store.created_clients().unwrap(),
        HashMap::from([
            (t.id.clone(), "omfp/web".to_owned()),
            (ws.id.clone(), "omfp-cli".to_owned()),
        ])
    );
}

// spec: ARC-009
#[test]
fn provenance_must_name_an_existing_thread() {
    let (_dir, store, ws) = with_workspace();
    let t = thread(Provenance::User, &ws.id);
    store.insert_thread(&t).unwrap();

    store.check_provenance(&Provenance::User).unwrap();
    store.check_provenance(&Provenance::Thread(t.id)).unwrap();
    let error = store
        .check_provenance(&Provenance::Thread("01JUNKNOWN".into()))
        .unwrap_err();
    assert!(matches!(error, StoreError::NotFound { .. }), "{error:?}");
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

// spec: DMN-075
#[test]
fn the_theme_is_a_setting_that_can_be_cleared() {
    let (_dir, store) = open();
    assert_eq!(store.theme().unwrap(), None);

    store
        .set_theme(Some(":root { --omfp-primary: red; }"))
        .unwrap();
    assert_eq!(
        store.theme().unwrap().as_deref(),
        Some(":root { --omfp-primary: red; }")
    );
    store.set_theme(None).unwrap();
    assert_eq!(store.theme().unwrap(), None);
}
