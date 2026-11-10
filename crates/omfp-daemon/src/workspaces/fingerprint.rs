//! The cheap check of a watched workspace (DMN-023): file metadata only, no git call, so it can run every couple of
//! seconds. A full scan runs only when the fingerprint changed.

use std::ffi::OsString;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// What the cheap check saw: the metadata (mtime, size, inode) of the folder, of its visible top-level entries (the
/// link itself and, followed, its target), and of `HEAD` and `index` in the git dir of the folder and of each entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint(u64);

/// The fingerprint of the workspace folder `folder` now; a missing folder has one too.
#[must_use]
pub fn fingerprint(folder: &Path) -> Fingerprint {
    let mut hasher = DefaultHasher::new();
    stamp(std::fs::metadata(folder)).hash(&mut hasher);
    git_files(folder, &mut hasher);
    let mut names: Vec<OsString> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(|entry| Some(entry.ok()?.file_name()))
        // Hidden entries (`.git`, `.omp`) are not repositories (DMN-022); the folder's own git dir is checked above.
        .filter(|name| !name.as_encoded_bytes().starts_with(b"."))
        .collect();
    names.sort_unstable();
    let mut entry = folder.to_path_buf();
    for name in names {
        entry.push(&name);
        name.hash(&mut hasher);
        let own = std::fs::symlink_metadata(&entry);
        let is_link = own.as_ref().is_ok_and(|meta| meta.file_type().is_symlink());
        stamp(own).hash(&mut hasher);
        if is_link {
            stamp(std::fs::metadata(&entry)).hash(&mut hasher);
        }
        git_files(&entry, &mut hasher);
        entry.pop();
    }
    Fingerprint(hasher.finish())
}

/// (mtime s, mtime ns, size, inode) of a path; `None` when it cannot be read.
fn stamp(meta: std::io::Result<std::fs::Metadata>) -> Option<(i64, i64, u64, u64)> {
    meta.ok()
        .map(|meta| (meta.mtime(), meta.mtime_nsec(), meta.size(), meta.ino()))
}

/// Hashes the stamps of `HEAD` and `index` in the git dir of `dir`, if it has one.
fn git_files(dir: &Path, hasher: &mut DefaultHasher) {
    let Some(git_dir) = git_dir(dir) else {
        return;
    };
    for file in ["HEAD", "index"] {
        stamp(std::fs::metadata(git_dir.join(file))).hash(hasher);
    }
}

/// The git dir of `dir` (links followed): `<dir>/.git` when it is a directory, else the `gitdir: <path>` of a `.git`
/// file (linked worktrees), relative to `dir`.
fn git_dir(dir: &Path) -> Option<PathBuf> {
    let dot_git = dir.join(".git");
    if std::fs::metadata(&dot_git).ok()?.is_dir() {
        return Some(dot_git);
    }
    let text = std::fs::read_to_string(&dot_git).ok()?;
    let target = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    Some(dir.join(target))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    // spec: DMN-023
    #[test]
    fn the_fingerprint_changes_with_entries_links_and_git_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("ws");
        let clone = folder.join("app");
        write(&clone.join(".git/HEAD"), "ref: refs/heads/main\n");
        write(&clone.join(".git/index"), "1");
        write(&clone.join("src/lib.rs"), "1");
        write(&folder.join(".omp/state"), "1");
        let main = dir.path().join("src/shop");
        write(
            &main.join(".git/worktrees/fix/HEAD"),
            "ref: refs/heads/fix\n",
        );
        write(&main.join(".git/worktrees/fix/index"), "1");
        write(&main.join(".git/worktrees/fix/logs/HEAD"), "1");
        let worktree = folder.join("fix");
        write(
            &worktree.join(".git"),
            &format!("gitdir: {}\n", main.join(".git/worktrees/fix").display()),
        );
        let linked = dir.path().join("src/lib");
        write(&linked.join(".git/index"), "1");
        std::os::unix::fs::symlink(&linked, folder.join("lib")).unwrap();
        let mut seen = fingerprint(&folder);
        assert_eq!(fingerprint(&folder), seen);

        let mut changed = |what: &str, change: &dyn Fn()| {
            change();
            let now = fingerprint(&folder);
            assert_ne!(now, seen, "{what}");
            seen = now;
        };
        changed("clone index", &|| write(&clone.join(".git/index"), "22"));
        changed("worktree HEAD", &|| {
            write(
                &main.join(".git/worktrees/fix/HEAD"),
                "ref: refs/heads/other\n",
            );
        });
        changed("linked index", &|| write(&linked.join(".git/index"), "22"));
        changed("new entry", &|| {
            std::os::unix::fs::symlink(&main, folder.join("shop")).unwrap();
        });

        // Files below the top level and outside the git files are not checked (sizes change, so a coarse clock does
        // not hide the changes).
        write(&clone.join("src/lib.rs"), "22");
        write(&main.join(".git/worktrees/fix/logs/HEAD"), "22");
        write(&folder.join(".omp/state"), "22");
        assert_eq!(fingerprint(&folder), seen);
    }
}
