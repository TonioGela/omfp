//! L2: path completion (`fs.complete`, DMN-021) over real directories in a temp dir.

use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use omfp_daemon::fs_complete::{Entry, MAX_ENTRIES, complete};
use omfp_testkit::TempRepo;
use tempfile::TempDir;

/// `<root>/src` holding `api` (a real repository), `apps/`, `.hidden/`, `alpha` (a symlink to `apps`), `notes.txt`
/// and a dangling link `gone`.
fn tree() -> (TempDir, PathBuf, TempRepo) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let src = root.join("src");
    std::fs::create_dir_all(src.join("apps")).unwrap();
    std::fs::create_dir_all(src.join(".hidden")).unwrap();
    std::fs::write(src.join("notes.txt"), "x").unwrap();
    symlink(src.join("apps"), src.join("alpha")).unwrap();
    symlink(src.join("missing"), src.join("gone")).unwrap();
    let repo = TempRepo::new().unwrap();
    symlink(repo.path(), src.join("api")).unwrap();
    (dir, root, repo)
}

fn names(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.name.as_str()).collect()
}

fn typed(path: &Path, suffix: &str) -> String {
    format!("{}{suffix}", path.display())
}

// spec: DMN-021
#[test]
fn a_trailing_slash_lists_the_children_and_marks_git_roots() {
    let (_dir, root, _repo) = tree();
    let entries = complete(&typed(&root, "/src/"), None);
    assert_eq!(names(&entries), ["alpha", "api", "apps"]);
    let api = &entries[1];
    assert_eq!(api.path, root.join("src/api"));
    assert!(api.is_git_root);
    assert!(!entries[2].is_git_root);
}

// spec: DMN-021
#[test]
fn a_partial_segment_lists_the_parents_children_starting_with_it() {
    let (_dir, root, _repo) = tree();
    assert_eq!(
        names(&complete(&typed(&root, "/src/ap"), None)),
        ["api", "apps"]
    );
    assert_eq!(names(&complete(&typed(&root, "/src/apps"), None)), ["apps"]);
    assert_eq!(
        names(&complete(&typed(&root, "/src/x"), None)),
        Vec::<&str>::new()
    );
    assert_eq!(
        names(&complete(&typed(&root, "/nowhere/"), None)),
        Vec::<&str>::new()
    );
}

// spec: DMN-021
#[test]
fn hidden_directories_only_when_the_segment_starts_with_a_dot() {
    let (_dir, root, _repo) = tree();
    assert_eq!(names(&complete(&typed(&root, "/src/."), None)), [".hidden"]);
    assert!(!names(&complete(&typed(&root, "/src/"), None)).contains(&".hidden"));
}

// spec: DMN-021
#[test]
fn a_leading_tilde_is_the_home_directory() {
    let (_dir, root, _repo) = tree();
    let home = root.join("src");
    assert_eq!(names(&complete("~/ap", Some(&home))), ["api", "apps"]);
    assert_eq!(complete("~/", Some(&home)), complete("~", Some(&home)));
    assert_eq!(complete("~/apps", Some(&home))[0].path, home.join("apps"));
    assert_eq!(complete("~/", None), Vec::new());
}

// spec: DMN-021
#[test]
fn at_most_the_cap_sorted_by_name() {
    let dir = TempDir::new().unwrap();
    for index in (0..MAX_ENTRIES + 5).rev() {
        std::fs::create_dir(dir.path().join(format!("d{index:04}"))).unwrap();
    }
    let entries = complete(&typed(dir.path(), "/"), None);
    assert_eq!(entries.len(), MAX_ENTRIES);
    assert_eq!(entries[0].name, "d0000");
    assert_eq!(
        entries[MAX_ENTRIES - 1].name,
        format!("d{:04}", MAX_ENTRIES - 1)
    );
}
