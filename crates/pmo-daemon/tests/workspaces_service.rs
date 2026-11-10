//! L1: the workspace service against the in-memory `FakeGit` (real SQLite file in a temp dir).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pmo_daemon::git::FakeGit;
use pmo_daemon::paths::DataDir;
use pmo_daemon::store::{Provenance, Store, StoreError, WorkspaceKind};
use pmo_daemon::threads::{NewThread, Threads};
use pmo_daemon::workspaces::{
    NewWorkspace, RemoveOptions, RepoRef, WorkspaceError, Workspaces, slugify,
};
use tempfile::TempDir;

struct Env {
    _dir: TempDir,
    root: PathBuf,
    data: DataDir,
    store: Arc<Store>,
    git: Arc<FakeGit>,
    workspaces: Workspaces,
    /// A fake repository on `main`.
    repo: PathBuf,
}

fn env() -> Env {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let data = DataDir::new(root.join("data"));
    std::fs::create_dir_all(data.root()).unwrap();
    let store = Arc::new(Store::open(&data.db()).unwrap());
    let git = Arc::new(FakeGit::new());
    let repo = fake_repo(&git, &root, "app");
    let workspaces = Workspaces::new(store.clone(), git.clone(), data.clone());
    Env {
        _dir: dir,
        root,
        data,
        store,
        git,
        workspaces,
        repo,
    }
}

fn fake_repo(git: &FakeGit, root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    std::fs::create_dir_all(&path).unwrap();
    git.add_repo(&path, "main");
    path
}

fn by_path(path: &Path) -> NewWorkspace {
    NewWorkspace::new(RepoRef::Path(path.to_owned()))
}

fn named(path: &Path, name: &str) -> NewWorkspace {
    NewWorkspace {
        name: Some(name.into()),
        ..by_path(path)
    }
}

fn worktree_branches(env: &Env) -> Vec<String> {
    env.git
        .worktrees(&env.repo)
        .into_iter()
        .map(|(_, branch)| branch)
        .collect()
}

// spec: DMN-020
#[test]
fn registering_requires_the_root_of_a_git_repository() {
    let env = env();
    let plain = env.root.join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let nested = env.repo.join("src");
    std::fs::create_dir_all(&nested).unwrap();

    assert!(matches!(
        env.workspaces.add_repository(&plain, None),
        Err(WorkspaceError::NotARepository(_))
    ));
    assert!(matches!(
        env.workspaces.add_repository(&nested, None),
        Err(WorkspaceError::NotARepositoryRoot(_))
    ));
    assert!(matches!(
        env.workspaces
            .add_repository(&env.root.join("missing"), None),
        Err(WorkspaceError::NotADirectory(_))
    ));
    assert_eq!(env.workspaces.repositories().unwrap(), Vec::new());
}

// spec: DMN-020
#[test]
fn a_registered_repository_records_name_base_ref_and_remote() {
    let env = env();
    env.git
        .set_remote_url(&env.repo, "git@example.com:org/app.git");

    let repo = env.workspaces.add_repository(&env.repo, None).unwrap();

    assert_eq!(repo.name, "app");
    assert_eq!(repo.path, env.repo);
    assert_eq!(repo.default_base_ref, "main");
    assert_eq!(
        repo.remote_url.as_deref(),
        Some("git@example.com:org/app.git")
    );
    assert_eq!(env.workspaces.repositories().unwrap(), vec![repo]);

    let explicit = fake_repo(&env.git, &env.root, "api");
    let api = env
        .workspaces
        .add_repository(&explicit, Some("develop"))
        .unwrap();
    assert_eq!(api.default_base_ref, "develop");
}

// spec: DMN-020
#[test]
fn a_repository_is_registered_once() {
    let env = env();
    env.workspaces.add_repository(&env.repo, None).unwrap();

    let error = env.workspaces.add_repository(&env.repo, None).unwrap_err();
    assert!(
        matches!(error, WorkspaceError::Store(StoreError::Conflict(_))),
        "{error:?}"
    );
}

// spec: DMN-021
#[test]
fn a_managed_workspace_gets_a_new_unique_pmo_branch() {
    let env = env();
    let repo = env.workspaces.add_repository(&env.repo, None).unwrap();
    env.git.add_branch(&env.repo, "pmo/fix-login-1");

    let first = env
        .workspaces
        .create(NewWorkspace {
            name: Some("Fix login!".into()),
            ..NewWorkspace::new(RepoRef::Id(repo.id.clone()))
        })
        .unwrap();
    let second = env
        .workspaces
        .create(named(&env.repo, "Fix login!"))
        .unwrap();

    assert_eq!(first.branch.as_deref(), Some("pmo/fix-login-2"));
    assert_eq!(second.branch.as_deref(), Some("pmo/fix-login-3"));
    assert_eq!(first.kind, WorkspaceKind::ManagedWorktree);
    assert_eq!(first.repository_id, Some(repo.id));
    assert_eq!(first.base_ref.as_deref(), Some("main"));
    assert_eq!(first.created_by, Provenance::User);
    assert_eq!(first.path, env.data.worktree("app", "fix-login-2"));
    assert!(first.path.is_dir());
    assert_eq!(
        env.git.worktrees(&env.repo),
        vec![
            (first.path.clone(), "pmo/fix-login-2".to_owned()),
            (second.path.clone(), "pmo/fix-login-3".to_owned()),
        ]
    );
    assert_eq!(env.workspaces.list().unwrap(), vec![first, second]);
}

// spec: DMN-021
#[test]
fn the_default_name_skips_existing_directories() {
    let env = env();
    std::fs::create_dir_all(env.data.worktree("app", "workspace-1")).unwrap();

    let ws = env.workspaces.create(by_path(&env.repo)).unwrap();

    assert_eq!(ws.branch.as_deref(), Some("pmo/workspace-2"));
    assert_eq!(ws.path, env.data.worktree("app", "workspace-2"));
}

// spec: DMN-020, DMN-021
#[test]
fn an_unregistered_repository_is_given_by_path() {
    let env = env();
    let nested = env.repo.join("src");
    std::fs::create_dir_all(&nested).unwrap();

    let ws = env
        .workspaces
        .create(NewWorkspace {
            base_ref: Some("main".into()),
            ..by_path(&nested)
        })
        .unwrap();

    assert_eq!(ws.repository_id, None);
    assert_eq!(ws.base_ref.as_deref(), Some("main"));
    assert_eq!(worktree_branches(&env), vec!["pmo/workspace-1"]);
}

// spec: DMN-021
#[test]
fn a_named_branch_is_checked_out_when_it_exists_and_created_otherwise() {
    let env = env();
    env.git.add_branch(&env.repo, "feature/login");

    let existing = env
        .workspaces
        .create(NewWorkspace {
            branch: Some("feature/login".into()),
            ..by_path(&env.repo)
        })
        .unwrap();
    let created = env
        .workspaces
        .create(NewWorkspace {
            branch: Some("topic".into()),
            ..by_path(&env.repo)
        })
        .unwrap();

    assert_eq!(existing.branch.as_deref(), Some("feature/login"));
    assert_eq!(existing.path, env.data.worktree("app", "feature-login"));
    assert_eq!(created.branch.as_deref(), Some("topic"));
    assert_eq!(
        env.git.branches(&env.repo),
        vec!["feature/login", "main", "topic"]
    );
    assert_eq!(worktree_branches(&env), vec!["feature/login", "topic"]);
}

#[test]
fn creating_in_a_directory_outside_git_fails() {
    let env = env();
    let plain = env.root.join("plain");
    std::fs::create_dir_all(&plain).unwrap();

    assert!(matches!(
        env.workspaces.create(by_path(&plain)),
        Err(WorkspaceError::NotARepository(_))
    ));
}

// spec: DMN-023
#[test]
fn any_directory_can_be_adopted_and_is_never_deleted() {
    let env = env();
    let plain = env.root.join("notes");
    std::fs::create_dir_all(&plain).unwrap();
    std::fs::write(plain.join("todo.txt"), "x").unwrap();

    let ws = env.workspaces.adopt(&plain, Provenance::User).unwrap();
    assert_eq!(ws.kind, WorkspaceKind::Adopted);
    assert_eq!(ws.path, plain);
    assert_eq!(ws.branch, None);

    let refused = env
        .workspaces
        .remove(
            &ws.id,
            RemoveOptions {
                force: true,
                ..RemoveOptions::default()
            },
        )
        .unwrap_err();
    assert!(matches!(refused, WorkspaceError::Adopted(_)), "{refused:?}");

    env.workspaces.forget(&ws.id, false).unwrap();
    assert_eq!(env.workspaces.list().unwrap(), Vec::new());
    assert!(plain.join("todo.txt").is_file());
}

// spec: DMN-023
#[test]
fn an_adopted_git_directory_records_its_branch_and_repository() {
    let env = env();
    let repo = env.workspaces.add_repository(&env.repo, None).unwrap();

    let ws = env.workspaces.adopt(&env.repo, Provenance::User).unwrap();

    assert_eq!(ws.kind, WorkspaceKind::Adopted);
    assert_eq!(ws.branch.as_deref(), Some("main"));
    assert_eq!(ws.repository_id, Some(repo.id));
    assert_eq!(ws.base_ref.as_deref(), Some("main"));
}

#[test]
fn a_directory_is_adopted_once() {
    let env = env();
    env.workspaces.adopt(&env.repo, Provenance::User).unwrap();

    assert!(matches!(
        env.workspaces.adopt(&env.repo, Provenance::User),
        Err(WorkspaceError::Store(StoreError::Conflict(_)))
    ));
}

// spec: DMN-024
#[test]
fn removing_a_dirty_workspace_needs_force() {
    let env = env();
    let ws = env.workspaces.create(by_path(&env.repo)).unwrap();
    env.git.set_dirty(&ws.path, true);

    let refused = env
        .workspaces
        .remove(&ws.id, RemoveOptions::default())
        .unwrap_err();
    assert!(matches!(refused, WorkspaceError::Dirty(_)), "{refused:?}");
    assert!(ws.path.is_dir());

    env.workspaces
        .remove(
            &ws.id,
            RemoveOptions {
                force: true,
                ..RemoveOptions::default()
            },
        )
        .unwrap();
    assert!(!ws.path.exists());
    assert_eq!(env.workspaces.list().unwrap(), Vec::new());
}

// spec: DMN-024
#[test]
fn a_workspace_used_by_a_live_thread_is_not_removed() {
    let env = env();
    let ws = env.workspaces.create(by_path(&env.repo)).unwrap();
    let in_use = RemoveOptions {
        force: true,
        in_use_by_live_thread: true,
        ..RemoveOptions::default()
    };

    let refused = env.workspaces.remove(&ws.id, in_use).unwrap_err();
    assert!(matches!(refused, WorkspaceError::InUse(_)), "{refused:?}");
    let refused = env.workspaces.forget(&ws.id, true).unwrap_err();
    assert!(matches!(refused, WorkspaceError::InUse(_)), "{refused:?}");
    assert!(ws.path.is_dir());
    assert_eq!(env.workspaces.list().unwrap(), vec![ws]);
}

// spec: DMN-024
#[test]
fn the_branch_is_kept_unless_deletion_is_requested() {
    let env = env();
    let kept = env.workspaces.create(named(&env.repo, "kept")).unwrap();
    let dropped = env.workspaces.create(named(&env.repo, "dropped")).unwrap();

    env.workspaces
        .remove(&kept.id, RemoveOptions::default())
        .unwrap();
    env.workspaces
        .remove(
            &dropped.id,
            RemoveOptions {
                delete_branch: true,
                ..RemoveOptions::default()
            },
        )
        .unwrap();

    assert_eq!(env.git.branches(&env.repo), vec!["main", "pmo/kept-1"]);
    assert_eq!(env.git.worktrees(&env.repo), Vec::new());
}

#[test]
fn a_removed_workspace_is_gone_for_later_operations() {
    let env = env();
    let ws = env.workspaces.create(by_path(&env.repo)).unwrap();
    env.workspaces
        .remove(&ws.id, RemoveOptions::default())
        .unwrap();

    assert!(matches!(
        env.workspaces.remove(&ws.id, RemoveOptions::default()),
        Err(WorkspaceError::Store(StoreError::NotFound { .. }))
    ));
    assert!(matches!(
        env.workspaces.status(&ws.id),
        Err(WorkspaceError::Store(StoreError::NotFound { .. }))
    ));
}

// spec: DMN-025
#[test]
fn git_operations_are_serialized_per_repository() {
    let env = env();
    let other = fake_repo(&env.git, &env.root, "api");
    let (entered_tx, entered) = mpsc::channel::<PathBuf>();
    let (release, release_rx) = mpsc::channel::<()>();
    let release_rx = Mutex::new(release_rx);
    let app = env.repo.clone();
    env.git.on_worktree_add(move |repo| {
        entered_tx.send(repo.to_owned()).unwrap();
        if repo == app {
            release_rx.lock().unwrap().recv().unwrap();
        }
    });
    let wait = Duration::from_secs(10);

    std::thread::scope(|scope| {
        let first = scope.spawn(|| env.workspaces.create(named(&env.repo, "a")));
        assert_eq!(entered.recv_timeout(wait), Ok(env.repo.clone()));
        let second = scope.spawn(|| env.workspaces.create(named(&env.repo, "b")));
        let parallel = scope.spawn(|| env.workspaces.create(named(&other, "c")));

        // Another repository proceeds while `app` is busy; the second `app` operation waits for the first.
        assert_eq!(entered.recv_timeout(wait), Ok(other.clone()));
        assert_eq!(
            entered.recv_timeout(Duration::from_millis(200)),
            Err(RecvTimeoutError::Timeout)
        );
        release.send(()).unwrap();
        assert_eq!(entered.recv_timeout(wait), Ok(env.repo.clone()));
        release.send(()).unwrap();

        for handle in [first, second, parallel] {
            handle.join().unwrap().unwrap();
        }
    });
    assert_eq!(worktree_branches(&env), vec!["pmo/a-1", "pmo/b-1"]);
}

// spec: DMN-026
#[test]
fn status_and_diff_are_reported_for_git_workspaces_only() {
    let env = env();
    let ws = env.workspaces.create(named(&env.repo, "x")).unwrap();
    env.git.set_dirty(&ws.path, true);
    let plain = env.root.join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let adopted = env.workspaces.adopt(&plain, Provenance::User).unwrap();

    let status = env.workspaces.status(&ws.id).unwrap().unwrap();
    assert_eq!(status.branch.as_deref(), Some("pmo/x-1"));
    assert_eq!((status.ahead, status.behind), (Some(0), Some(0)));
    assert!(status.dirty);
    assert_eq!(env.workspaces.diff(&ws.id).unwrap(), Some(String::new()));

    assert_eq!(env.workspaces.status(&adopted.id).unwrap(), None);
    assert_eq!(env.workspaces.diff(&adopted.id).unwrap(), None);
}

// spec: DMN-027
#[test]
fn threads_sharing_a_workspace_are_reported() {
    let env = env();
    let threads = Threads::new(env.store.clone(), env.data.clone());
    let ws = env.workspaces.create(by_path(&env.repo)).unwrap();
    let a = threads
        .create(NewThread {
            workspace_ids: vec![ws.id.clone()],
            ..NewThread::new("a")
        })
        .unwrap();
    let b = threads.create(NewThread::new("b")).unwrap();
    threads.attach(&b.id, &ws.id, None).unwrap();

    assert_eq!(
        env.workspaces.threads_using(&ws.id).unwrap(),
        vec![a.id, b.id]
    );
}

// spec: ARC-009
#[test]
fn workspace_provenance_is_recorded() {
    let env = env();
    let threads = Threads::new(env.store.clone(), env.data.clone());
    let creator = threads.create(NewThread::new("agent")).unwrap();

    let ws = env
        .workspaces
        .create(NewWorkspace {
            created_by: Provenance::Thread(creator.id.clone()),
            ..by_path(&env.repo)
        })
        .unwrap();
    assert_eq!(
        env.workspaces.get(&ws.id).unwrap().created_by,
        Provenance::Thread(creator.id)
    );

    let unknown = env.workspaces.adopt(
        &env.repo,
        Provenance::Thread("01JUNKNOWNTHREAD0000000000".into()),
    );
    assert!(matches!(
        unknown,
        Err(WorkspaceError::Store(StoreError::NotFound { .. }))
    ));
}

#[test]
fn slugs_are_lowercase_ascii_words() {
    assert_eq!(slugify("Fix login!"), "fix-login");
    assert_eq!(slugify("  feature/Login--Page "), "feature-login-page");
    assert_eq!(slugify("été"), "t");
    assert_eq!(slugify("!!!"), "workspace");
    assert_eq!(slugify(&"a".repeat(100)).len(), 40);
}
