//! Files of a workspace (DMN-089, DMN-090): reading one with its hash and its text at `HEAD`, writing one unless it
//! changed on disk since it was read.
//!
//! Paths are relative to the workspace folder or absolute; `..` is resolved without following links. A relative path
//! may not leave the folder, while links inside it (linked repositories) are followed: they are the workspace's. An
//! absolute path outside the folder is read as it is, read-only (DMN-310): the daemon's agents already read anything
//! the user may (D-17), and writes stay in the folder (DMN-313).

use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use omfp_protocol::{FileContent, FileKind, FileWritten};
use sha1::{Digest, Sha1};

use super::{Discovered, Result, WorkspaceError, Workspaces};
use crate::paths::replace_file;

/// Text beyond this many bytes is cut at the last line end before it (DMN-089).
pub const TEXT_CAP: usize = 2 * 1024 * 1024;
/// Images up to this many bytes carry their content (DMN-089).
pub const IMAGE_CAP: usize = 8 * 1024 * 1024;

/// Image extensions (lower case) and their MIME types.
const IMAGES: [(&str, &str); 9] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("avif", "image/avif"),
    ("bmp", "image/bmp"),
    ("ico", "image/x-icon"),
    ("svg", "image/svg+xml"),
];

/// How many links a write follows to find the file it replaces.
const MAX_LINKS: usize = 40;

/// Bytes read at a time.
const CHUNK: usize = 64 * 1024;

/// The MIME type of an image file, by its extension (case-insensitive); `None` for other files.
#[must_use]
pub fn image_mime(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    IMAGES
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, mime)| *mime)
}

/// `path` without `.` and with `..` applied to the components before it, links not followed; `None` when a `..` goes
/// above the start of a relative path or above `/`.
fn normalize(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => normalized.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                if !matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    return None;
                }
                normalized.pop();
            }
            Component::Normal(name) => normalized.push(name),
        }
    }
    Some(normalized)
}

/// Where a requested path points (DMN-089, DMN-310).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Resolved {
    /// A file of the workspace folder, relative to it.
    Inside(PathBuf),
    /// An absolute path outside the workspace folder, `..` resolved: read-only (DMN-310).
    Outside(PathBuf),
}

/// The file `path` of the workspace `folder` (DMN-089, DMN-310): relative to the folder when inside it (an absolute
/// path inside it too), else an absolute path as it is; `bad_request` when a relative path leaves the folder, or for
/// the folder itself.
pub(super) fn resolve(folder: &Path, path: &str) -> Result<Resolved> {
    let outside = || WorkspaceError::OutsideFolder(path.to_owned());
    let requested = Path::new(path);
    let normalized = normalize(requested).ok_or_else(outside)?;
    let relative = if requested.is_absolute() {
        let canonical = std::fs::canonicalize(folder).ok();
        let inside = [Some(folder), canonical.as_deref()]
            .into_iter()
            .flatten()
            .find_map(|folder| normalized.strip_prefix(folder).ok());
        match inside {
            Some(relative) => relative.to_owned(),
            None => return Ok(Resolved::Outside(normalized)),
        }
    } else {
        normalized
    };
    if relative.as_os_str().is_empty() {
        return Err(WorkspaceError::IsADirectory(path.to_owned()));
    }
    Ok(Resolved::Inside(relative))
}

/// The repository holding a file, where its `HEAD` version is read (DMN-089, DMN-311).
struct Holder {
    /// A folder of the repository's working tree that `inside` is relative to.
    dir: PathBuf,
    /// The name the result gives the repository.
    name: String,
    /// The file's path relative to `dir`.
    inside: PathBuf,
}

/// The repository holding `relative` (a path in the workspace folder) and the path inside it: the repository whose
/// entry is its first component, else the folder's own repository.
fn holder(repos: &[Discovered], relative: &Path) -> Option<Holder> {
    let held = |repo: &Discovered, inside: &Path| Holder {
        dir: repo.path.clone(),
        name: repo.name.clone(),
        inside: inside.to_owned(),
    };
    let mut components = relative.components();
    let first = components.next()?.as_os_str();
    repos
        .iter()
        .find(|repo| !repo.is_root && first == repo.name.as_str())
        .map(|repo| held(repo, components.as_path()))
        .or_else(|| {
            repos
                .iter()
                .find(|repo| repo.is_root)
                .map(|repo| held(repo, relative))
        })
}

/// Where a requested file is.
struct Located {
    /// The file on disk.
    full: PathBuf,
    /// The path the results name: relative to the workspace folder, or absolute outside it (DMN-310).
    shown: PathBuf,
    /// The workspace's repositories the last scan found; `None` for a file outside the folder.
    repos: Option<Vec<Discovered>>,
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// What one pass over a file found.
struct Scanned {
    size: u64,
    hash: String,
    /// The first bytes, up to the `keep` of [`scan`].
    start: Vec<u8>,
    /// Valid UTF-8 without NUL bytes.
    text: bool,
}

/// Reads `file` once: its size, hash, first `keep` bytes and whether it is text, without holding all of it.
fn scan(file: &mut File, keep: usize) -> std::io::Result<Scanned> {
    let mut hasher = Sha1::new();
    let mut size: u64 = 0;
    let mut start = Vec::new();
    let mut text = true;
    // The tail of a character cut by the previous read (at most 3 bytes) stays in front of the next one.
    let mut buffer = vec![0_u8; CHUNK + 3];
    let mut carried = 0;
    loop {
        let read = file.read(&mut buffer[carried..])?;
        if read == 0 {
            break;
        }
        let fresh = &buffer[carried..carried + read];
        hasher.update(fresh);
        size += read as u64;
        let room = keep.saturating_sub(start.len());
        start.extend_from_slice(&fresh[..room.min(read)]);
        if !text {
            continue;
        }
        if fresh.contains(&0) {
            text = false;
            carried = 0;
            continue;
        }
        let end = carried + read;
        carried = match std::str::from_utf8(&buffer[..end]) {
            Ok(_) => 0,
            Err(error) if error.error_len().is_none() => {
                let valid = error.valid_up_to();
                buffer.copy_within(valid..end, 0);
                end - valid
            }
            Err(_) => {
                text = false;
                0
            }
        };
    }
    Ok(Scanned {
        size,
        hash: hex::encode(hasher.finalize()),
        start,
        text: text && carried == 0,
    })
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha1::digest(bytes))
}

/// The text to show of a file of `size` bytes starting with `start` (valid UTF-8): all of it, or its part before the
/// last line end within [`TEXT_CAP`] (`true`: cut).
fn shown_text(start: &[u8], size: u64) -> (String, bool) {
    if size <= TEXT_CAP as u64 {
        return (String::from_utf8_lossy(start).into_owned(), false);
    }
    let capped = &start[..TEXT_CAP.min(start.len())];
    let end = match capped.iter().rposition(|byte| *byte == b'\n') {
        Some(line_end) => line_end + 1,
        // One line longer than the cap: cut at the last whole character.
        None => match std::str::from_utf8(capped) {
            Ok(_) => capped.len(),
            Err(error) => error.valid_up_to(),
        },
    };
    (String::from_utf8_lossy(&capped[..end]).into_owned(), true)
}

/// The error of a path that does not exist (a file in its way counts as missing).
fn missing(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
    )
}

/// Opens an existing file for reading: `not_found` when missing, `bad_request` for a directory or a file the daemon's
/// user may not read (DMN-315).
fn open(full: &Path, path: &str) -> Result<File> {
    let file = File::open(full).map_err(|error| {
        if missing(&error) {
            WorkspaceError::NoSuchFile(path.to_owned())
        } else if error.kind() == std::io::ErrorKind::PermissionDenied {
            WorkspaceError::Unreadable(path.to_owned())
        } else {
            error.into()
        }
    })?;
    if file.metadata()?.is_dir() {
        return Err(WorkspaceError::IsADirectory(path.to_owned()));
    }
    Ok(file)
}

/// Where a new file at `full` goes: through links (even dangling ones) to the file they name, in a folder that
/// exists (no folder is created).
fn new_file_target(full: &Path, path: &str) -> Result<PathBuf> {
    let mut target = full.to_owned();
    for _ in 0..MAX_LINKS {
        let Ok(meta) = std::fs::symlink_metadata(&target) else {
            break;
        };
        if !meta.file_type().is_symlink() {
            break;
        }
        let link = std::fs::read_link(&target)?;
        target = target
            .parent()
            .map_or_else(|| link.clone(), |parent| parent.join(&link));
    }
    let missing_folder = || WorkspaceError::NoSuchFolder(path.to_owned());
    let (Some(parent), Some(name)) = (target.parent(), target.file_name()) else {
        return Err(missing_folder());
    };
    let parent = std::fs::canonicalize(parent).map_err(|_| missing_folder())?;
    if !parent.is_dir() {
        return Err(missing_folder());
    }
    Ok(parent.join(name))
}

impl Workspaces {
    /// Where the file `path` of workspace `id` is (DMN-089, DMN-310). A relative path the folder does not hold is
    /// looked up in each repository, in scan order; an absolute path outside the folder is taken as it is.
    fn locate(&self, id: &str, path: &str) -> Result<Located> {
        let ws = self.get(id)?;
        let mut relative = match resolve(&ws.path, path)? {
            Resolved::Inside(relative) => relative,
            Resolved::Outside(absolute) => {
                return Ok(Located {
                    full: absolute.clone(),
                    shown: absolute,
                    repos: None,
                });
            }
        };
        let repos = self.discovered(id)?;
        if Path::new(path).is_relative() && std::fs::metadata(ws.path.join(&relative)).is_err() {
            let found = repos
                .iter()
                .filter(|repo| !repo.is_root)
                .map(|repo| Path::new(&repo.name).join(&relative))
                .find(|candidate| std::fs::metadata(ws.path.join(candidate)).is_ok());
            if let Some(found) = found {
                relative = found;
            }
        }
        Ok(Located {
            full: ws.path.join(&relative),
            shown: relative,
            repos: Some(repos),
        })
    }

    /// The repository holding a located file: a repository of the workspace (DMN-089) or, outside the folder, the
    /// git working tree holding it (DMN-311); `None` outside every repository.
    fn holder_of(&self, located: &Located) -> Option<Holder> {
        if let Some(repos) = &located.repos {
            return holder(repos, &located.shown);
        }
        let dir = located.full.parent()?;
        // A folder that is missing, or that git cannot tell about, is outside every repository.
        let top = self.git.toplevel(dir).ok()??;
        Some(Holder {
            dir: dir.to_owned(),
            name: top.file_name()?.to_string_lossy().into_owned(),
            inside: PathBuf::from(located.full.file_name()?),
        })
    }

    /// The bytes of a held file at `HEAD` of its repository; `None` when `HEAD` does not have them.
    fn at_head(&self, holder: &Holder) -> Result<Option<Vec<u8>>> {
        match holder.inside.to_str() {
            Some(inside) => Ok(self.git.show_head(&holder.dir, inside)?),
            None => Ok(None),
        }
    }

    /// The file `path` of workspace `id`, opened for reading its raw bytes (DMN-250, DMN-314), with the path
    /// [`Self::read_file`] names: resolved, refused and looked up in the repositories as it does.
    pub fn open_file(&self, id: &str, path: &str) -> Result<(File, PathBuf)> {
        let located = self.locate(id, path)?;
        let file = open(&located.full, path)?;
        Ok((file, located.shown))
    }

    /// The bytes of the file `path` of workspace `id` at `HEAD` of the repository holding it (DMN-255, DMN-311), with
    /// the path [`Self::read_file`] names: resolved as it does, though the file need not exist any more; `None` for a
    /// file `HEAD` does not have or outside every repository.
    pub fn read_head(&self, id: &str, path: &str) -> Result<Option<(Vec<u8>, PathBuf)>> {
        let located = self.locate(id, path)?;
        let Some(holder) = self.holder_of(&located) else {
            return Ok(None);
        };
        Ok(self.at_head(&holder)?.map(|bytes| (bytes, located.shown)))
    }

    /// The file `path` of workspace `id` (DMN-089), or, for an absolute path outside its folder, that file, read-only
    /// (DMN-310). A relative path the folder does not hold is looked up in each repository, in scan order. With
    /// `known_hash` equal to the file's hash the result is `unchanged`, without its content or `HEAD` text.
    pub fn read_file(&self, id: &str, path: &str, known_hash: Option<&str>) -> Result<FileContent> {
        let located = self.locate(id, path)?;
        let mut file = open(&located.full, path)?;
        let mime = image_mime(&located.shown);
        let keep = if mime.is_some() { IMAGE_CAP } else { TEXT_CAP };
        let scanned = scan(&mut file, keep)?;
        let kind = if mime.is_some() {
            FileKind::Image
        } else if scanned.text {
            FileKind::Text
        } else {
            FileKind::Binary
        };
        let unchanged = known_hash == Some(scanned.hash.as_str());
        let (content, truncated) = match kind {
            _ if unchanged => (None, false),
            FileKind::Text => {
                let (text, truncated) = shown_text(&scanned.start, scanned.size);
                (Some(text), truncated)
            }
            FileKind::Image => (
                (scanned.size <= IMAGE_CAP as u64).then(|| BASE64.encode(&scanned.start)),
                false,
            ),
            FileKind::Binary => (None, false),
        };
        let outside = located.repos.is_none();
        // Outside the folder, finding the repository runs git: an unchanged file, polled, skips it.
        let holder = if outside && unchanged {
            None
        } else {
            self.holder_of(&located)
        };
        let (head, head_hash) = match &holder {
            Some(holder) if !unchanged => match kind {
                FileKind::Text if !truncated => (
                    self.at_head(holder)?
                        .and_then(|bytes| String::from_utf8(bytes).ok()),
                    None,
                ),
                FileKind::Image => (None, self.at_head(holder)?.map(|bytes| hash(&bytes))),
                _ => (None, None),
            },
            _ => (None, None),
        };
        Ok(FileContent {
            path: text(&located.shown),
            kind,
            size: scanned.size,
            hash: scanned.hash,
            unchanged,
            content,
            truncated,
            mime: mime.map(str::to_owned),
            repo: holder.map(|holder| holder.name),
            head,
            head_hash,
            outside,
        })
    }

    /// Writes `content` to the file `path` of workspace `id` (DMN-090) only when it still has `expected_hash` or,
    /// without one, when it does not exist; otherwise `conflict` and nothing is written. The write is atomic, keeps
    /// the file's permissions, goes through links and creates no folder. A path outside the workspace folder is
    /// refused, absolute or not: files there are read-only (DMN-313).
    pub fn write_file(
        &self,
        id: &str,
        path: &str,
        content: &str,
        expected_hash: Option<&str>,
    ) -> Result<FileWritten> {
        let ws = self.get(id)?;
        let relative = match resolve(&ws.path, path)? {
            Resolved::Inside(relative) => relative,
            Resolved::Outside(_) => return Err(WorkspaceError::ReadOnly(path.to_owned())),
        };
        let full = ws.path.join(&relative);
        let (target, permissions) = match std::fs::canonicalize(&full) {
            Ok(target) => {
                let mut file = open(&target, path)?;
                let permissions = file.metadata()?.permissions();
                let current = scan(&mut file, 0)?.hash;
                match expected_hash {
                    Some(expected) if expected == current => {}
                    Some(_) => {
                        return Err(WorkspaceError::FileChanged(format!(
                            "{path} changed on disk since it was read"
                        )));
                    }
                    None => {
                        return Err(WorkspaceError::FileChanged(format!(
                            "{path} already exists"
                        )));
                    }
                }
                (target, Some(permissions))
            }
            Err(error) if missing(&error) => {
                if expected_hash.is_some() {
                    return Err(WorkspaceError::FileChanged(format!(
                        "{path} no longer exists"
                    )));
                }
                (new_file_target(&full, path)?, None)
            }
            Err(error) => return Err(error.into()),
        };
        replace_file(&target, content.as_bytes(), permissions)?;
        Ok(FileWritten {
            path: text(&relative),
            hash: hash(content.as_bytes()),
            size: content.len() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: DMN-089, DMN-310, DMN-312
    #[test]
    fn dot_dot_is_resolved_lexically_and_only_absolute_paths_may_leave_the_folder() {
        let folder = Path::new("/w/ws");
        let inside = |relative: &str| Resolved::Inside(PathBuf::from(relative));
        assert_eq!(resolve(folder, "a/./b/../c").unwrap(), inside("a/c"));
        assert_eq!(resolve(folder, "/w/ws/a/../b").unwrap(), inside("b"));
        for relative in ["../x", "a/../../x", "..", "/.."] {
            assert!(
                matches!(
                    resolve(folder, relative),
                    Err(WorkspaceError::OutsideFolder(_))
                ),
                "{relative}"
            );
        }
        for (absolute, resolved) in [
            ("/w/other/x", "/w/other/x"),
            ("/w/ws/../x", "/w/x"),
            ("/w/wsx/./y", "/w/wsx/y"),
        ] {
            assert_eq!(
                resolve(folder, absolute).unwrap(),
                Resolved::Outside(PathBuf::from(resolved)),
                "{absolute}"
            );
        }
        assert!(matches!(
            resolve(folder, "a/.."),
            Err(WorkspaceError::IsADirectory(_))
        ));
    }

    // spec: DMN-089
    #[test]
    fn long_text_is_cut_at_the_last_line_end_before_the_cap() {
        let mut bytes = "line\n".repeat(TEXT_CAP / 5).into_bytes();
        bytes.extend_from_slice(b"tail without end");
        let size = bytes.len() as u64;
        let (text, truncated) = shown_text(&bytes[..TEXT_CAP], size);
        assert!(truncated);
        assert!(
            text.ends_with('\n') && text.len() <= TEXT_CAP,
            "{}",
            text.len()
        );
        assert_eq!(text.len(), TEXT_CAP / 5 * 5);

        let (text, truncated) = shown_text(b"short\n", 6);
        assert_eq!((text.as_str(), truncated), ("short\n", false));
    }

    // spec: DMN-089
    #[test]
    fn a_scan_tells_text_across_reads_and_hashes_every_byte() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        // A multi-byte character straddling the read size is still text.
        let mut bytes = vec![b'a'; CHUNK - 1];
        bytes.extend_from_slice("é".as_bytes());
        bytes.extend_from_slice(b"z");
        std::fs::write(&path, &bytes).unwrap();
        let scanned = scan(&mut File::open(&path).unwrap(), 4).unwrap();
        assert!(scanned.text);
        assert_eq!(scanned.size, bytes.len() as u64);
        assert_eq!(scanned.hash, hash(&bytes));
        assert_eq!(scanned.start, b"aaaa");

        std::fs::write(&path, b"a\0b").unwrap();
        assert!(!scan(&mut File::open(&path).unwrap(), 0).unwrap().text);
        std::fs::write(&path, [b'a', 0xc3]).unwrap();
        assert!(!scan(&mut File::open(&path).unwrap(), 0).unwrap().text);
    }
}
