//! Repositories and workspaces (02-daemon, "Repositories and workspaces"): the optional repository registry,
//! daemon-managed worktrees, adopted directories, and their git status.
//!
//! Calls are blocking; the server layer runs them on blocking tasks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::git::{Git, GitError, GitStatus, WorktreeBranch};
use crate::paths::DataDir;
use crate::store::{
    Provenance, Repository, Store, StoreError, Workspace, WorkspaceKind, new_id, now_ms,
};

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("{} is not a directory", .0.display())]
    NotADirectory(PathBuf),
    #[error("{} is not in a git repository", .0.display())]
    NotARepository(PathBuf),
    #[error("{} is not the root of a git repository", .0.display())]
    NotARepositoryRoot(PathBuf),
    #[error("{} has a detached HEAD: give a base ref", .0.display())]
    DetachedHead(PathBuf),
    #[error("workspace {0} has uncommitted changes")]
    Dirty(String),
    #[error("workspace {0} is used by a running thread")]
    InUse(String),
    #[error("workspace {0} is adopted: it can only be forgotten, never deleted")]
    Adopted(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = WorkspaceError> = std::result::Result<T, E>;

/// The repository a managed workspace is created in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoRef {
    /// A registered repository.
    Id(String),
    /// Any directory inside a git repository; registration is optional (DMN-020).
    Path(PathBuf),
}

/// Parameters of [`Workspaces::create`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWorkspace {
    pub repo: RepoRef,
    /// An existing local branch is checked out; another name creates that branch. Default: a new
    /// `pmo/<slug>-<n>` branch, unique in the repository.
    pub branch: Option<String>,
    /// Base of a new branch and reference for ahead/behind. Default: the repository's default base ref, or the
    /// branch checked out in the repository.
    pub base_ref: Option<String>,
    /// Human name the default branch and directory derive from (default: `workspace`).
    pub name: Option<String>,
    pub created_by: Provenance,
}

impl NewWorkspace {
    /// A workspace in `repo` with every default.
    #[must_use]
    pub fn new(repo: RepoRef) -> Self {
        Self {
            repo,
            branch: None,
            base_ref: None,
            name: None,
            created_by: Provenance::User,
        }
    }
}

/// Options of [`Workspaces::remove`] (DMN-024).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RemoveOptions {
    /// Remove even with uncommitted changes.
    pub force: bool,
    /// Also delete the workspace's branch.
    pub delete_branch: bool,
    /// A thread with a live omp process uses the workspace (known by the caller); removal is then refused.
    pub in_use_by_live_thread: bool,
}

/// Turns free text into a directory/branch-friendly slug: lowercase ASCII letters and digits separated by single
/// dashes, at most 40 characters; `workspace` when nothing is left.
#[must_use]
pub fn slugify(text: &str) -> String {
    const MAX_LEN: usize = 40;
    let mut slug = String::new();
    for c in text.chars() {
        if slug.len() == MAX_LEN {
            break;
        }
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "workspace".to_owned()
    } else {
        slug.to_owned()
    }
}

/// Repository registry and workspace lifecycle.
#[derive(Debug)]
pub struct Workspaces {
    store: Arc<Store>,
    git: Arc<dyn Git>,
    data: DataDir,
    /// One lock per repository (keyed by its main working tree), held during git operations that mutate the shared
    /// `.git` (DMN-025).
    repo_locks: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl Workspaces {
    #[must_use]
    pub fn new(store: Arc<Store>, git: Arc<dyn Git>, data: DataDir) -> Self {
        Self {
            store,
            git,
            data,
            repo_locks: Mutex::new(HashMap::new()),
        }
    }

    fn repo_lock(&self, repo_root: &Path) -> Arc<Mutex<()>> {
        self.repo_locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(repo_root.to_owned())
            .or_default()
            .clone()
    }

    // Repositories

    /// Registers the git repository rooted at `path` (DMN-020).
    pub fn add_repository(
        &self,
        path: &Path,
        default_base_ref: Option<&str>,
    ) -> Result<Repository> {
        let path = existing_dir(path)?;
        let toplevel = self
            .git
            .toplevel(&path)?
            .ok_or_else(|| WorkspaceError::NotARepository(path.clone()))?;
        if std::fs::canonicalize(toplevel)? != path {
            return Err(WorkspaceError::NotARepositoryRoot(path));
        }
        let default_base_ref = match default_base_ref {
            Some(base) => base.to_owned(),
            None => self
                .git
                .current_branch(&path)?
                .ok_or_else(|| WorkspaceError::DetachedHead(path.clone()))?,
        };
        let repo = Repository {
            id: new_id(),
            name: dir_name(&path),
            remote_url: self.git.remote_url(&path, "origin")?,
            path,
            default_base_ref,
            created_at: now_ms(),
        };
        self.store.insert_repository(&repo)?;
        Ok(repo)
    }

    /// Unregisters a repository; its workspaces stay.
    pub fn remove_repository(&self, id: &str) -> Result<()> {
        Ok(self.store.delete_repository(id)?)
    }

    pub fn repositories(&self) -> Result<Vec<Repository>> {
        Ok(self.store.repositories()?)
    }

    // Workspaces

    /// A live workspace.
    pub fn get(&self, id: &str) -> Result<Workspace> {
        let ws = self.store.workspace(id)?;
        if ws.removed_at.is_some() {
            return Err(StoreError::NotFound {
                entity: "workspace",
                id: id.to_owned(),
            }
            .into());
        }
        Ok(ws)
    }

    /// Live workspaces.
    pub fn list(&self) -> Result<Vec<Workspace>> {
        Ok(self.store.workspaces()?)
    }

    /// Creates a daemon-managed git worktree under `worktrees/<repo-slug>/<workspace-slug>` (DMN-021).
    pub fn create(&self, new: NewWorkspace) -> Result<Workspace> {
        self.store.check_provenance(&new.created_by)?;
        let (toplevel, registered) = match &new.repo {
            RepoRef::Id(id) => {
                let repo = self.store.repository(id)?;
                (repo.path.clone(), Some(repo))
            }
            RepoRef::Path(path) => {
                let dir = existing_dir(path)?;
                let toplevel = self
                    .git
                    .toplevel(&dir)?
                    .ok_or(WorkspaceError::NotARepository(dir))?;
                let toplevel = std::fs::canonicalize(toplevel)?;
                let registered = self.store.repository_by_path(&toplevel)?;
                (toplevel, registered)
            }
        };
        let base_ref = match (new.base_ref, &registered) {
            (Some(base), _) => base,
            (None, Some(repo)) => repo.default_base_ref.clone(),
            (None, None) => self
                .git
                .current_branch(&toplevel)?
                .ok_or_else(|| WorkspaceError::DetachedHead(toplevel.clone()))?,
        };
        let repo_slug = slugify(
            &registered
                .as_ref()
                .map_or_else(|| dir_name(&toplevel), |repo| repo.name.clone()),
        );

        let lock = self.repo_lock(&self.git.main_root(&toplevel)?);
        let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
        let (path, spec) = if let Some(branch) = new.branch {
            let slug = slugify(new.name.as_deref().unwrap_or(&branch));
            let path = self.free_worktree_dir(&repo_slug, &slug)?;
            let spec = if self.git.branch_exists(&toplevel, &branch)? {
                WorktreeBranch::Existing(branch)
            } else {
                WorktreeBranch::New {
                    name: branch,
                    base_ref: base_ref.clone(),
                }
            };
            (path, spec)
        } else {
            let slug = slugify(new.name.as_deref().unwrap_or("workspace"));
            self.default_branch(&toplevel, &repo_slug, &slug, &base_ref)?
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        self.git.worktree_add(&toplevel, &path, &spec)?;
        let branch = match &spec {
            WorktreeBranch::New { name, .. } | WorktreeBranch::Existing(name) => name.clone(),
        };
        let ws = Workspace {
            id: new_id(),
            path: std::fs::canonicalize(&path)?,
            kind: WorkspaceKind::ManagedWorktree,
            repository_id: registered.map(|repo| repo.id),
            branch: Some(branch),
            base_ref: Some(base_ref),
            pr_url: None,
            pr_number: None,
            created_by: new.created_by,
            created_at: now_ms(),
            removed_at: None,
        };
        if let Err(error) = self.store.insert_workspace(&ws) {
            // Best effort: do not leave an untracked worktree (or a branch we created) behind.
            let _ = self.git.worktree_remove(&toplevel, &ws.path, true);
            if let WorktreeBranch::New { name, .. } = &spec {
                let _ = self.git.delete_branch(&toplevel, name);
            }
            return Err(error.into());
        }
        Ok(ws)
    }

    /// The first `<dir>/<slug>`, `<dir>/<slug>-2`, … that does not exist.
    fn free_worktree_dir(&self, repo_slug: &str, slug: &str) -> Result<PathBuf> {
        for n in 1_u32.. {
            let name = if n == 1 {
                slug.to_owned()
            } else {
                format!("{slug}-{n}")
            };
            let path = std::path::absolute(self.data.worktree(repo_slug, &name))?;
            if !path.exists() {
                return Ok(path);
            }
        }
        Err(std::io::Error::other("no free worktree directory name").into())
    }

    /// The first `pmo/<slug>-<n>` (n ≥ 1) whose branch and directory are both free.
    fn default_branch(
        &self,
        repo: &Path,
        repo_slug: &str,
        slug: &str,
        base_ref: &str,
    ) -> Result<(PathBuf, WorktreeBranch)> {
        for n in 1_u32.. {
            let name = format!("{slug}-{n}");
            let branch = format!("pmo/{name}");
            let path = std::path::absolute(self.data.worktree(repo_slug, &name))?;
            if !path.exists() && !self.git.branch_exists(repo, &branch)? {
                return Ok((
                    path,
                    WorktreeBranch::New {
                        name: branch,
                        base_ref: base_ref.to_owned(),
                    },
                ));
            }
        }
        Err(std::io::Error::other("no free branch name").into())
    }

    /// Records an existing directory, git or not, as an adopted workspace (DMN-023).
    pub fn adopt(&self, path: &Path, created_by: Provenance) -> Result<Workspace> {
        self.store.check_provenance(&created_by)?;
        let path = existing_dir(path)?;
        let (repository, branch) = if self.git.toplevel(&path)?.is_some() {
            let root = std::fs::canonicalize(self.git.main_root(&path)?)?;
            (
                self.store.repository_by_path(&root)?,
                self.git.current_branch(&path)?,
            )
        } else {
            (None, None)
        };
        let ws = Workspace {
            id: new_id(),
            path,
            kind: WorkspaceKind::Adopted,
            base_ref: repository
                .as_ref()
                .map(|repo| repo.default_base_ref.clone()),
            repository_id: repository.map(|repo| repo.id),
            branch,
            pr_url: None,
            pr_number: None,
            created_by,
            created_at: now_ms(),
            removed_at: None,
        };
        self.store.insert_workspace(&ws)?;
        Ok(ws)
    }

    /// Deletes a managed worktree (DMN-024). Refused for adopted workspaces, while a live thread uses it, and with
    /// uncommitted changes unless forced. The branch is kept unless `delete_branch`. A worktree directory that no
    /// longer exists is only forgotten.
    pub fn remove(&self, id: &str, options: RemoveOptions) -> Result<()> {
        let ws = self.get(id)?;
        if ws.kind == WorkspaceKind::Adopted {
            return Err(WorkspaceError::Adopted(ws.id));
        }
        if options.in_use_by_live_thread {
            return Err(WorkspaceError::InUse(ws.id));
        }
        if ws.path.exists() {
            let root = self.git.main_root(&ws.path)?;
            let lock = self.repo_lock(&root);
            let _guard = lock.lock().unwrap_or_else(PoisonError::into_inner);
            if !options.force && self.git.is_dirty(&ws.path)? {
                return Err(WorkspaceError::Dirty(ws.id));
            }
            self.git.worktree_remove(&root, &ws.path, options.force)?;
            if let (true, Some(branch)) = (options.delete_branch, &ws.branch) {
                self.git.delete_branch(&root, branch)?;
            }
        }
        self.store.mark_workspace_removed(&ws.id, now_ms())?;
        Ok(())
    }

    /// Drops the record of a workspace and detaches it from its threads; the directory is left untouched.
    pub fn forget(&self, id: &str, in_use_by_live_thread: bool) -> Result<()> {
        let ws = self.get(id)?;
        if in_use_by_live_thread {
            return Err(WorkspaceError::InUse(ws.id));
        }
        self.store.mark_workspace_removed(&ws.id, now_ms())?;
        Ok(())
    }

    /// Git status of a workspace; `None` for a directory outside git (DMN-026).
    pub fn status(&self, id: &str) -> Result<Option<GitStatus>> {
        let ws = self.get(id)?;
        if self.git.toplevel(&ws.path)?.is_none() {
            return Ok(None);
        }
        Ok(Some(self.git.status(&ws.path, ws.base_ref.as_deref())?))
    }

    /// Changes against the base ref, uncommitted ones included; `None` for a directory outside git (DMN-026).
    pub fn diff(&self, id: &str) -> Result<Option<String>> {
        let ws = self.get(id)?;
        if self.git.toplevel(&ws.path)?.is_none() {
            return Ok(None);
        }
        Ok(Some(self.git.diff(&ws.path, ws.base_ref.as_deref())?))
    }

    /// Ids of the threads the workspace is attached to (DMN-027). Sharing is allowed; this lets clients warn.
    pub fn threads_using(&self, id: &str) -> Result<Vec<String>> {
        let ws = self.get(id)?;
        Ok(self.store.workspace_thread_ids(&ws.id)?)
    }
}

fn existing_dir(path: &Path) -> Result<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(path) if path.is_dir() => Ok(path),
        _ => Err(WorkspaceError::NotADirectory(path.to_owned())),
    }
}

fn dir_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || "repository".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}
