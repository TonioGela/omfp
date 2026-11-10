//! In-memory [`Git`] for component tests (L1). Repositories are real directories registered with
//! [`FakeGit::add_repo`]; branches, worktrees and dirtiness are simulated. `worktree_add`/`worktree_remove` create and
//! delete the worktree directory, like git does.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::{Git, GitError, GitStatus, Result, WorktreeBranch};

type Hook = Arc<dyn Fn(&Path) + Send + Sync>;

#[derive(Debug, Default)]
struct FakeRepo {
    head: Option<String>,
    branches: BTreeSet<String>,
    remote_url: Option<String>,
    /// Linked worktree path → checked-out branch.
    worktrees: BTreeMap<PathBuf, String>,
}

#[derive(Debug, Default)]
struct State {
    repos: BTreeMap<PathBuf, FakeRepo>,
    dirty: BTreeSet<PathBuf>,
}

impl State {
    /// The repository root and, for a linked worktree, its path.
    fn locate(&self, dir: &Path) -> Option<(&PathBuf, Option<&PathBuf>)> {
        self.repos.iter().find_map(|(root, repo)| {
            if let Some(worktree) = repo.worktrees.keys().find(|wt| dir.starts_with(wt)) {
                Some((root, Some(worktree)))
            } else {
                dir.starts_with(root).then_some((root, None))
            }
        })
    }

    fn repo(&mut self, root: &Path) -> Result<&mut FakeRepo> {
        self.repos
            .get_mut(root)
            .ok_or_else(|| GitError::NotARepository(root.to_owned()))
    }
}

#[derive(Default)]
pub struct FakeGit {
    state: Mutex<State>,
    on_worktree_add: Mutex<Option<Hook>>,
}

impl fmt::Debug for FakeGit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeGit")
            .field("state", &*self.state())
            .finish_non_exhaustive()
    }
}

fn failed(command: &str, stderr: impl Into<String>) -> GitError {
    GitError::Failed {
        command: command.to_owned(),
        stderr: stderr.into(),
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

impl FakeGit {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Makes `root` (an existing directory) a repository with `head` checked out.
    pub fn add_repo(&self, root: &Path, head: &str) {
        self.state().repos.insert(
            canonical(root),
            FakeRepo {
                head: Some(head.to_owned()),
                branches: BTreeSet::from([head.to_owned()]),
                ..FakeRepo::default()
            },
        );
    }

    pub fn add_branch(&self, root: &Path, branch: &str) {
        if let Some(repo) = self.state().repos.get_mut(&canonical(root)) {
            repo.branches.insert(branch.to_owned());
        }
    }

    pub fn set_remote_url(&self, root: &Path, url: &str) {
        if let Some(repo) = self.state().repos.get_mut(&canonical(root)) {
            repo.remote_url = Some(url.to_owned());
        }
    }

    pub fn set_dirty(&self, dir: &Path, dirty: bool) {
        let dir = canonical(dir);
        let mut state = self.state();
        if dirty {
            state.dirty.insert(dir);
        } else {
            state.dirty.remove(&dir);
        }
    }

    /// Local branches of the repository at `root`.
    #[must_use]
    pub fn branches(&self, root: &Path) -> Vec<String> {
        self.state()
            .repos
            .get(&canonical(root))
            .map(|repo| repo.branches.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Linked worktrees of the repository at `root`, with their branches.
    #[must_use]
    pub fn worktrees(&self, root: &Path) -> Vec<(PathBuf, String)> {
        self.state()
            .repos
            .get(&canonical(root))
            .map(|repo| {
                repo.worktrees
                    .iter()
                    .map(|(path, branch)| (path.clone(), branch.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Calls `hook` with the repository path at the start of every `worktree_add`, outside the fake's own locks.
    pub fn on_worktree_add(&self, hook: impl Fn(&Path) + Send + Sync + 'static) {
        *self
            .on_worktree_add
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(hook));
    }
}

impl Git for FakeGit {
    fn toplevel(&self, dir: &Path) -> Result<Option<PathBuf>> {
        Ok(self
            .state()
            .locate(dir)
            .map(|(root, worktree)| worktree.unwrap_or(root).clone()))
    }

    fn main_root(&self, dir: &Path) -> Result<PathBuf> {
        self.state()
            .locate(dir)
            .map(|(root, _)| root.clone())
            .ok_or_else(|| GitError::NotARepository(dir.to_owned()))
    }

    fn current_branch(&self, dir: &Path) -> Result<Option<String>> {
        let state = self.state();
        let (root, worktree) = state
            .locate(dir)
            .ok_or_else(|| GitError::NotARepository(dir.to_owned()))?;
        let repo = &state.repos[root];
        Ok(match worktree {
            Some(worktree) => Some(repo.worktrees[worktree].clone()),
            None => repo.head.clone(),
        })
    }

    fn remote_url(&self, repo: &Path, _remote: &str) -> Result<Option<String>> {
        Ok(self.state().repo(repo)?.remote_url.clone())
    }

    fn branch_exists(&self, repo: &Path, branch: &str) -> Result<bool> {
        Ok(self.state().repo(repo)?.branches.contains(branch))
    }

    fn worktree_add(&self, repo: &Path, path: &Path, branch: &WorktreeBranch) -> Result<()> {
        let hook = self
            .on_worktree_add
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(hook) = hook {
            hook(repo);
        }
        let mut state = self.state();
        let fake = state.repo(repo)?;
        if path.exists() {
            return Err(failed(
                "worktree add",
                format!("'{}' already exists", path.display()),
            ));
        }
        let name = match branch {
            WorktreeBranch::New { name, base_ref } => {
                if fake.branches.contains(name) {
                    return Err(failed(
                        "worktree add",
                        format!("a branch named '{name}' already exists"),
                    ));
                }
                if !fake.branches.contains(base_ref) {
                    return Err(failed(
                        "worktree add",
                        format!("invalid reference: {base_ref}"),
                    ));
                }
                fake.branches.insert(name.clone());
                name
            }
            WorktreeBranch::Existing(name) => {
                let checked_out = fake.head.as_ref() == Some(name)
                    || fake.worktrees.values().any(|branch| branch == name);
                if !fake.branches.contains(name) || checked_out {
                    return Err(failed("worktree add", format!("cannot check out '{name}'")));
                }
                name
            }
        };
        std::fs::create_dir_all(path)?;
        fake.worktrees.insert(canonical(path), name.clone());
        Ok(())
    }

    fn worktree_remove(&self, repo: &Path, path: &Path, force: bool) -> Result<()> {
        let mut state = self.state();
        let path = canonical(path);
        if state.dirty.contains(&path) && !force {
            return Err(failed(
                "worktree remove",
                "contains modified or untracked files, use --force",
            ));
        }
        if state.repo(repo)?.worktrees.remove(&path).is_none() {
            return Err(failed(
                "worktree remove",
                format!("'{}' is not a working tree", path.display()),
            ));
        }
        state.dirty.remove(&path);
        std::fs::remove_dir_all(&path)?;
        Ok(())
    }

    fn delete_branch(&self, repo: &Path, branch: &str) -> Result<()> {
        let mut state = self.state();
        let fake = state.repo(repo)?;
        let checked_out = fake.head.as_deref() == Some(branch)
            || fake.worktrees.values().any(|name| name == branch);
        if checked_out || !fake.branches.remove(branch) {
            return Err(failed(
                "branch --delete",
                format!("cannot delete branch '{branch}'"),
            ));
        }
        Ok(())
    }

    fn is_dirty(&self, dir: &Path) -> Result<bool> {
        Ok(self.state().dirty.contains(dir))
    }

    fn status(&self, dir: &Path, base_ref: Option<&str>) -> Result<GitStatus> {
        Ok(GitStatus {
            branch: self.current_branch(dir)?,
            ahead: base_ref.map(|_| 0),
            behind: base_ref.map(|_| 0),
            dirty: self.is_dirty(dir)?,
            files: Vec::new(),
        })
    }

    fn diff(&self, dir: &Path, _base_ref: Option<&str>) -> Result<String> {
        self.main_root(dir).map(|_| String::new())
    }
}
