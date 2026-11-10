//! L2: the workspace service and `GitCli` against real git repositories in temp dirs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pmo_daemon::git::{ChangedFile, Git, GitCli};
use pmo_daemon::paths::DataDir;
use pmo_daemon::store::{Provenance, Store};
use pmo_daemon::workspaces::{NewWorkspace, RemoveOptions, RepoRef, WorkspaceError, Workspaces};
use pmo_testkit::TempRepo;
use tempfile::TempDir;

struct Env {
    _data_dir: TempDir,
    repo: TempRepo,
    /// Canonical path of `repo`.
    root: PathBuf,
    workspaces: Workspaces,
}

fn env() -> Env {
    let data_dir = TempDir::new().unwrap();
    let data = DataDir::new(data_dir.path().canonicalize().unwrap());
    let store = Arc::new(Store::open(&data.db()).unwrap());
    let repo = TempRepo::new().unwrap();
    let root = repo.path().canonicalize().unwrap();
    Env {
        _data_dir: data_dir,
        repo,
        root,
        workspaces: Workspaces::new(store, Arc::new(GitCli::default()), data),
    }
}

impl Env {
    /// Runs git with the test identity in `dir`.
    fn git_in(&self, dir: &Path, args: &[&str]) -> String {
        let mut full = vec!["-C", dir.to_str().unwrap()];
        full.extend(args);
        self.repo.git(&full).unwrap()
    }

    fn commit_in(&self, dir: &Path, file: &str, contents: &str) {
        std::fs::write(dir.join(file), contents).unwrap();
        self.git_in(dir, &["add", "--", file]);
        self.git_in(dir, &["commit", "--quiet", "--message", file]);
    }

    fn create(&self, name: &str) -> pmo_daemon::store::Workspace {
        self.workspaces
            .create(NewWorkspace {
                name: Some(name.into()),
                ..NewWorkspace::new(RepoRef::Path(self.root.clone()))
            })
            .unwrap()
    }
}

// spec: DMN-020
#[test]
fn only_a_real_repository_root_is_registered() {
    let env = env();
    let plain = TempDir::new().unwrap();
    std::fs::create_dir_all(env.root.join("src")).unwrap();

    assert!(matches!(
        env.workspaces.add_repository(plain.path(), None),
        Err(WorkspaceError::NotARepository(_))
    ));
    assert!(matches!(
        env.workspaces.add_repository(&env.root.join("src"), None),
        Err(WorkspaceError::NotARepositoryRoot(_))
    ));

    env.repo
        .git(&["remote", "add", "origin", "https://example.invalid/app.git"])
        .unwrap();
    let repo = env
        .workspaces
        .add_repository(env.repo.path(), None)
        .unwrap();
    assert_eq!(repo.path, env.root);
    assert_eq!(repo.default_base_ref, "main");
    assert_eq!(
        repo.remote_url.as_deref(),
        Some("https://example.invalid/app.git")
    );
}

// spec: DMN-021
#[test]
fn a_managed_worktree_is_created_on_disk_on_its_branch() {
    let env = env();
    let repo = env.workspaces.add_repository(&env.root, None).unwrap();

    let ws = env
        .workspaces
        .create(NewWorkspace {
            name: Some("Fix login".into()),
            ..NewWorkspace::new(RepoRef::Id(repo.id))
        })
        .unwrap();

    assert_eq!(ws.branch.as_deref(), Some("pmo/fix-login-1"));
    assert!(ws.path.join("README.md").is_file());
    assert_eq!(
        env.git_in(&ws.path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "pmo/fix-login-1"
    );
    assert!(
        env.repo
            .git(&["worktree", "list", "--porcelain"])
            .unwrap()
            .contains(&format!("worktree {}", ws.path.display()))
    );
    // A second workspace with the same name gets the next free branch.
    assert_eq!(
        env.create("Fix login").branch.as_deref(),
        Some("pmo/fix-login-2")
    );
}

// spec: DMN-021
#[test]
fn an_existing_branch_is_checked_out() {
    let env = env();
    env.repo.git(&["branch", "feature"]).unwrap();

    let ws = env
        .workspaces
        .create(NewWorkspace {
            branch: Some("feature".into()),
            ..NewWorkspace::new(RepoRef::Path(env.root.clone()))
        })
        .unwrap();

    assert_eq!(
        env.git_in(&ws.path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "feature"
    );
}

// spec: DMN-026
#[test]
fn status_and_diff_cover_commits_since_the_base_and_uncommitted_changes() {
    let env = env();
    let ws = env.create("x");
    env.commit_in(&ws.path, "feature.txt", "a\nb\n");
    env.repo
        .commit_file("other.txt", "main only\n", "advance main")
        .unwrap();
    std::fs::write(ws.path.join("README.md"), "test repository\nmore\n").unwrap();
    std::fs::write(ws.path.join("new.txt"), "x\n").unwrap();

    let status = env.workspaces.status(&ws.id).unwrap().unwrap();
    assert_eq!(status.branch.as_deref(), Some("pmo/x-1"));
    assert_eq!((status.ahead, status.behind), (Some(1), Some(1)));
    assert!(status.dirty);
    let file = |path: &str, additions| ChangedFile {
        path: path.into(),
        additions: Some(additions),
        deletions: Some(0),
    };
    assert_eq!(
        status.files,
        vec![
            file("README.md", 1),
            file("feature.txt", 2),
            file("new.txt", 1)
        ]
    );

    let diff = env.workspaces.diff(&ws.id).unwrap().unwrap();
    for expected in ["+++ b/feature.txt", "+more", "+++ b/new.txt"] {
        assert!(
            diff.contains(expected),
            "{expected:?} missing from:\n{diff}"
        );
    }
    assert!(!diff.contains("other.txt"), "{diff}");
}

// spec: DMN-024
#[test]
fn a_dirty_worktree_is_removed_only_when_forced_and_its_branch_is_kept() {
    let env = env();
    let ws = env.create("x");
    std::fs::write(ws.path.join("scratch.txt"), "wip\n").unwrap();

    assert!(matches!(
        env.workspaces.remove(&ws.id, RemoveOptions::default()),
        Err(WorkspaceError::Dirty(_))
    ));
    assert!(ws.path.join("scratch.txt").is_file());

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
    assert!(
        !env.repo
            .git(&["worktree", "list", "--porcelain"])
            .unwrap()
            .contains(&ws.path.display().to_string())
    );
    assert_eq!(
        env.repo.git(&["branch", "--list", "pmo/x-1"]).unwrap(),
        "pmo/x-1"
    );
}

// spec: DMN-024
#[test]
fn the_branch_is_deleted_on_request() {
    let env = env();
    let ws = env.create("x");
    env.commit_in(&ws.path, "unmerged.txt", "work\n");

    env.workspaces
        .remove(
            &ws.id,
            RemoveOptions {
                delete_branch: true,
                ..RemoveOptions::default()
            },
        )
        .unwrap();

    assert_eq!(env.repo.git(&["branch", "--list", "pmo/x-1"]).unwrap(), "");
}

// spec: DMN-023
#[test]
fn an_adopted_checkout_is_never_deleted() {
    let env = env();
    let ws = env.workspaces.adopt(&env.root, Provenance::User).unwrap();
    assert_eq!(ws.branch.as_deref(), Some("main"));

    assert!(matches!(
        env.workspaces.remove(
            &ws.id,
            RemoveOptions {
                force: true,
                ..RemoveOptions::default()
            }
        ),
        Err(WorkspaceError::Adopted(_))
    ));
    env.workspaces.forget(&ws.id, false).unwrap();

    assert!(env.root.join(".git").exists());
    assert!(env.root.join("README.md").is_file());
}

#[test]
fn git_cli_locates_repositories_and_worktrees() {
    let env = env();
    let git = GitCli::default();
    let plain = TempDir::new().unwrap();
    let ws = env.create("x");

    assert_eq!(git.toplevel(plain.path()).unwrap(), None);
    assert_eq!(git.toplevel(&env.root).unwrap(), Some(env.root.clone()));
    assert_eq!(git.toplevel(&ws.path).unwrap(), Some(ws.path.clone()));
    assert_eq!(git.main_root(&ws.path).unwrap(), env.root);
    assert_eq!(
        git.current_branch(&env.root).unwrap().as_deref(),
        Some("main")
    );
    assert!(git.branch_exists(&env.root, "pmo/x-1").unwrap());
    assert!(!git.branch_exists(&env.root, "nope").unwrap());
    assert!(!git.is_dirty(&ws.path).unwrap());
    std::fs::write(ws.path.join("README.md"), "changed\n").unwrap();
    assert!(git.is_dirty(&ws.path).unwrap());
    env.git_in(&env.root, &["checkout", "--quiet", "--detach"]);
    assert_eq!(git.current_branch(&env.root).unwrap(), None);
}
