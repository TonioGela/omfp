//! Path completion on the daemon host (DMN-021): the child directories of a typed path, so a client can type a
//! repository or folder path with completion even when it runs on another machine.

use std::path::{Path, PathBuf};

/// At most this many entries are returned.
pub const MAX_ENTRIES: usize = 200;

/// A child directory of the completed path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// `~` expanded.
    pub path: PathBuf,
    /// The directory is the root of a git repository (it holds `.git`).
    pub is_git_root: bool,
}

/// The directories completing `typed`: the children of `typed` when it ends with `/`, else the children of its parent
/// whose name starts with its last segment. A leading `~` is `home`. Hidden directories are listed only when the typed
/// segment starts with `.`; symlinks to directories count as directories. Sorted by name, at most [`MAX_ENTRIES`].
/// Paths that are not absolute (after `~`), and directories that cannot be read, complete to nothing.
#[must_use]
pub fn complete(typed: &str, home: Option<&Path>) -> Vec<Entry> {
    let Some(expanded) = expand_home(typed, home) else {
        return Vec::new();
    };
    // Split on the text, not with `Path`, which drops a trailing `.` segment.
    let text = expanded.to_string_lossy();
    let (dir, prefix) = match text.rfind('/') {
        Some(slash) if typed != "~" => (
            PathBuf::from(if slash == 0 { "/" } else { &text[..slash] }),
            text[slash + 1..].to_owned(),
        ),
        _ => (expanded.clone(), String::new()),
    };
    if !dir.is_absolute() {
        return Vec::new();
    }
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let show_hidden = prefix.starts_with('.');
    let mut entries: Vec<Entry> = read
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with(&prefix) || (name.starts_with('.') && !show_hidden) {
                return None;
            }
            let path = entry.path();
            // `is_dir` follows symlinks.
            path.is_dir().then(|| Entry {
                is_git_root: path.join(".git").exists(),
                name,
                path,
            })
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries.truncate(MAX_ENTRIES);
    entries
}

/// `typed` with a leading `~` (alone or before `/`) replaced by `home`; `None` when it needs a home and has none.
fn expand_home(typed: &str, home: Option<&Path>) -> Option<PathBuf> {
    match typed.strip_prefix('~') {
        Some("") => home.map(Path::to_owned),
        Some(rest) if rest.starts_with('/') => {
            home.map(|home| home.join(rest.trim_start_matches('/')))
        }
        _ => Some(PathBuf::from(typed)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: DMN-021
    #[test]
    fn home_is_expanded_only_as_a_leading_segment() {
        let home = Path::new("/home/dev");
        assert_eq!(expand_home("~", Some(home)), Some(home.to_owned()));
        assert_eq!(
            expand_home("~/src/", Some(home)),
            Some(PathBuf::from("/home/dev/src/"))
        );
        assert_eq!(expand_home("~/x", None), None);
        assert_eq!(
            expand_home("~other/x", Some(home)),
            Some(PathBuf::from("~other/x"))
        );
        assert_eq!(
            expand_home("/a/~/b", Some(home)),
            Some(PathBuf::from("/a/~/b"))
        );
    }

    // spec: DMN-021
    #[test]
    fn relative_paths_complete_to_nothing() {
        assert_eq!(complete("src/", None), Vec::new());
        assert_eq!(complete("", None), Vec::new());
    }
}
