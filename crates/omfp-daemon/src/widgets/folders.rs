//! Widget folders (AGT-012, AGT-013, AGT-023, DMN-082): which folders are widgets, their entry file, and a version
//! that changes whenever a file of the folder is saved, read from file metadata only.

use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use omfp_protocol::{WidgetInfo, WidgetSource};

/// Entry files in order of preference: the first present is the widget's entry (AGT-012).
pub const ENTRIES: [&str; 4] = ["index.tsx", "index.ts", "index.jsx", "index.js"];

/// Whether `name` can name a widget folder: ASCII letters, digits, `_`, `-` and `.`, not starting with `.`
/// (AGT-012).
#[must_use]
pub fn is_widget_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// The widget folders of `library`, by name; nothing when it does not exist.
#[must_use]
pub fn scan(library: &Path, source: WidgetSource) -> Vec<WidgetInfo> {
    let Ok(entries) = std::fs::read_dir(library) else {
        return Vec::new();
    };
    let mut widgets: Vec<WidgetInfo> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let folder = entry.path();
            (is_widget_name(&name) && folder.is_dir()).then(|| WidgetInfo {
                source,
                version: version(&folder),
                entry: entry_name(&folder),
                name,
            })
        })
        .collect();
    widgets.sort_by(|a, b| a.name.cmp(&b.name));
    widgets
}

/// The folder's entry file name, if it has one (AGT-012).
#[must_use]
pub fn entry(folder: &Path) -> Option<&'static str> {
    ENTRIES
        .into_iter()
        .find(|entry| folder.join(entry).is_file())
}

fn entry_name(folder: &Path) -> Option<String> {
    entry(folder).map(str::to_owned)
}

/// Every file below `folder` with its metadata, by relative path. Symlinks to files count as files (with their
/// target's metadata); symlinks to directories are not followed.
#[must_use]
pub fn files(folder: &Path) -> Vec<(PathBuf, Metadata)> {
    fn walk(folder: &Path, relative: &Path, out: &mut Vec<(PathBuf, Metadata)>) {
        let Ok(entries) = std::fs::read_dir(folder.join(relative)) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = relative.join(entry.file_name());
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                walk(folder, &path, out);
            } else if let Ok(metadata) = std::fs::metadata(entry.path())
                && metadata.is_file()
            {
                out.push((path, metadata));
            }
        }
    }
    let mut out = Vec::new();
    walk(folder, Path::new(""), &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A hash of the relative path, size and modification time of every file of the folder (DMN-082): any saved change
/// makes a new version; no file content is read.
#[must_use]
pub fn version(folder: &Path) -> String {
    // FNV-1a: stable across Rust releases and daemon restarts, unlike `DefaultHasher`.
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let mut hash = OFFSET;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(PRIME);
        }
    };
    for (path, metadata) in files(folder) {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_nanos());
        feed(path.as_os_str().as_encoded_bytes());
        feed(&[0]);
        feed(&metadata.len().to_le_bytes());
        feed(&modified.to_le_bytes());
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use std::fs::FileTimes;
    use std::time::Duration;

    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    // spec: AGT-012
    #[test]
    fn widget_names_are_letters_digits_underscore_dash_and_inner_dots() {
        for good in ["tests", "my-widget_2", "v1.2", "A"] {
            assert!(is_widget_name(good), "{good:?}");
        }
        for bad in ["", ".", "..", ".hidden", "a b", "a/b", "é", "a:b"] {
            assert!(!is_widget_name(bad), "{bad:?}");
        }
    }

    // spec: AGT-012, DMN-082
    #[test]
    fn folders_are_listed_by_name_with_the_first_present_entry() {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path();
        write(&library.join("b/index.js"), "");
        write(&library.join("b/index.ts"), "");
        write(&library.join("a/index.jsx"), "");
        write(&library.join("c/readme.md"), "");
        write(&library.join(".hidden/index.ts"), "");
        write(&library.join("bad name/index.ts"), "");
        write(&library.join("file.ts"), "");

        let widgets = scan(library, WidgetSource::User);

        let listed: Vec<(&str, Option<&str>)> = widgets
            .iter()
            .map(|widget| (widget.name.as_str(), widget.entry.as_deref()))
            .collect();
        assert_eq!(
            listed,
            [
                ("a", Some("index.jsx")),
                ("b", Some("index.ts")),
                ("c", None)
            ]
        );
        assert!(widgets.iter().all(|w| w.source == WidgetSource::User));
        assert_eq!(scan(&library.join("missing"), WidgetSource::User), []);
    }

    // spec: AGT-013, DMN-082
    #[test]
    fn the_version_changes_with_any_saved_file_and_only_then() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path();
        write(&folder.join("index.tsx"), "export default 1;");
        write(&folder.join("lib/chart.ts"), "export const a = 1;");
        let first = version(folder);
        assert_eq!(version(folder), first, "stable without change");
        assert_eq!(first.len(), 16);

        // Same size, new modification time (a save).
        let file = std::fs::File::options()
            .write(true)
            .open(folder.join("lib/chart.ts"))
            .unwrap();
        let later = std::fs::metadata(folder.join("lib/chart.ts"))
            .unwrap()
            .modified()
            .unwrap()
            + Duration::from_secs(5);
        file.set_times(FileTimes::new().set_modified(later))
            .unwrap();
        let saved = version(folder);
        assert_ne!(saved, first);

        write(&folder.join("lib/new.ts"), "");
        let added = version(folder);
        assert_ne!(added, saved);
        std::fs::remove_file(folder.join("lib/new.ts")).unwrap();
        assert_eq!(version(folder), saved, "back to the same files");
    }
}
