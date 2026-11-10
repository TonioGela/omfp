//! Discovering the repositories of a workspace folder (DMN-022): the folder itself when it is a git root, and every
//! top-level entry that is a git root, symlinks followed. Hidden entries (`.omp`, `.git`, …) and non-git entries are
//! ignored; nothing deeper is looked at.

use std::path::{Path, PathBuf};

use crate::git::{Git, Result};

/// How a discovered repository sits in its folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoKind {
    /// A symlink; `target` as written in the link.
    Link { target: PathBuf },
    /// A linked worktree of the repository whose main working tree is `main`.
    Worktree { main: PathBuf },
    /// A repository of its own.
    Clone,
}

/// A repository found in a workspace folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovered {
    /// Entry name; the folder's own name for the root.
    pub name: String,
    /// Path inside the workspace (never a symlink's target).
    pub path: PathBuf,
    pub kind: RepoKind,
    pub branch: Option<String>,
    pub head: Option<String>,
    /// The workspace folder itself.
    pub is_root: bool,
    /// `path` with symlinks resolved.
    pub canonical: PathBuf,
}

/// The repositories of `folder`: the folder itself first when it is a git root, then the entries by name.
pub fn scan(git: &dyn Git, folder: &Path) -> Result<Vec<Discovered>> {
    let mut found = Vec::new();
    let folder_name = folder
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    if let Some(root) = inspect(git, folder, folder_name, true)? {
        found.push(root);
    }
    let mut entries: Vec<(String, PathBuf)> = std::fs::read_dir(folder)?
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            (!name.starts_with('.')).then(|| (name, entry.path()))
        })
        .collect();
    entries.sort();
    for (name, path) in entries {
        // `is_dir` follows symlinks: links to files and dangling links are skipped.
        if path.is_dir()
            && let Some(repo) = inspect(git, &path, name, false)?
        {
            found.push(repo);
        }
    }
    Ok(found)
}

/// `Some` when `path` is the root of a git working tree.
fn inspect(git: &dyn Git, path: &Path, name: String, is_root: bool) -> Result<Option<Discovered>> {
    let canonical = std::fs::canonicalize(path)?;
    let Some(toplevel) = git.toplevel(path)? else {
        return Ok(None);
    };
    if std::fs::canonicalize(&toplevel)? != canonical {
        return Ok(None);
    }
    let kind = if !is_root && std::fs::symlink_metadata(path)?.file_type().is_symlink() {
        RepoKind::Link {
            target: std::fs::read_link(path)?,
        }
    } else {
        let main = std::fs::canonicalize(git.main_root(path)?)?;
        if main == canonical {
            RepoKind::Clone
        } else {
            RepoKind::Worktree { main }
        }
    };
    Ok(Some(Discovered {
        name,
        path: path.to_owned(),
        kind,
        branch: git.current_branch(path)?,
        head: git.head(path)?,
        is_root,
        canonical,
    }))
}
