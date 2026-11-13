//! Layout of the daemon's data directory (00-overview, "On-disk data"), and keeping it private to its owner
//! (DMN-180): the directory `0700`, the files the daemon and the desktop write in it `0600`.

use std::fs::{File, OpenOptions, Permissions};
use std::io::ErrorKind;
use std::os::unix::fs::{
    DirBuilderExt as _, MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _,
};
use std::path::{Path, PathBuf};

/// The daemon's data directory and the paths inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// SQLite database.
    #[must_use]
    pub fn db(&self) -> PathBuf {
        self.root.join("omfp.db")
    }

    /// Access token file (DMN-003).
    #[must_use]
    pub fn token(&self) -> PathBuf {
        self.root.join("token")
    }

    /// Everything owned by one thread.
    #[must_use]
    pub fn thread(&self, thread_id: &str) -> PathBuf {
        self.root.join("threads").join(thread_id)
    }

    /// The thread's omp session directory (ARC-020).
    #[must_use]
    pub fn thread_sessions(&self, thread_id: &str) -> PathBuf {
        self.thread(thread_id).join("sessions")
    }

    /// The thread's omp stderr log (DMN-036).
    #[must_use]
    pub fn thread_log(&self, thread_id: &str) -> PathBuf {
        self.thread(thread_id).join("omp.log")
    }

    /// The user's widget library, `user:<name>` (AGT-023).
    #[must_use]
    pub fn widgets(&self) -> PathBuf {
        self.root.join("widgets")
    }

    /// The daemon's omp extension root, passed to every omp as `--extension` (DMN-107); it holds only `agents/`.
    #[must_use]
    pub fn omp_extension(&self) -> PathBuf {
        self.root.join("omp")
    }

    /// The `widget-builder` agent definition (AGT-029, DMN-107).
    #[must_use]
    pub fn widget_builder(&self) -> PathBuf {
        self.omp_extension()
            .join("agents")
            .join("widget-builder.md")
    }

    /// The working directory, session directory and stderr log of the short-lived omp of `auth.*` (DMN-219).
    #[must_use]
    pub fn login(&self) -> PathBuf {
        self.root.join("login")
    }
}

/// Group and other permission bits.
const SHARED_BITS: u32 = 0o077;

/// Creates `dir` (its missing parents with the default mode) with mode `0700` (DMN-180). An existing directory this
/// user owns that group or others may access is tightened to `0700`; its old mode is returned so the caller can say
/// so. A directory of another user is left as it is.
pub fn create_private_dir(dir: &Path) -> std::io::Result<Option<u32>> {
    let create = || std::fs::DirBuilder::new().mode(0o700).create(dir);
    let created = match create() {
        Err(error) if error.kind() == ErrorKind::NotFound => {
            if let Some(parent) = dir.parent() {
                std::fs::create_dir_all(parent)?;
            }
            create()
        }
        other => other,
    };
    match created {
        Ok(()) => Ok(None),
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            let metadata = std::fs::metadata(dir)?;
            if !metadata.is_dir() {
                return Err(std::io::Error::new(
                    ErrorKind::AlreadyExists,
                    format!("{} is not a directory", dir.display()),
                ));
            }
            if metadata.uid() != rustix::process::getuid().as_raw() {
                return Ok(None);
            }
            tighten(dir, &metadata, 0o700)
        }
        Err(error) => Err(error),
    }
}

/// Restricts the file at `path` to this user (`0600`) when group or others may access it, returning its old mode
/// (DMN-003, DMN-180); a file of another user is an error, since only its owner could fix it.
pub fn make_private_file(path: &Path) -> std::io::Result<Option<u32>> {
    let metadata = std::fs::metadata(path)?;
    if metadata.uid() != rustix::process::getuid().as_raw() {
        return Err(std::io::Error::new(
            ErrorKind::PermissionDenied,
            format!("{} belongs to another user", path.display()),
        ));
    }
    tighten(path, &metadata, 0o600)
}

fn tighten(path: &Path, metadata: &std::fs::Metadata, mode: u32) -> std::io::Result<Option<u32>> {
    let old = metadata.permissions().mode() & 0o7777;
    if old & SHARED_BITS == 0 {
        return Ok(None);
    }
    std::fs::set_permissions(path, Permissions::from_mode(mode))?;
    Ok(Some(old))
}

/// [`OpenOptions`] creating a file with mode `0600` (DMN-180).
#[must_use]
pub fn private_file_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.mode(0o600);
    options
}

/// Opens the log file `path` for appending, created `0600`; an existing one others may read is tightened first
/// (DMN-180): `daemon.log`, a thread's `omp.log`.
pub fn open_private_log(path: &Path) -> std::io::Result<File> {
    let file = private_file_options()
        .create(true)
        .append(true)
        .open(path)?;
    let metadata = file.metadata()?;
    if metadata.permissions().mode() & SHARED_BITS != 0 {
        file.set_permissions(Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_on_disk_layout() {
        let data = DataDir::new("/data/omfp");
        assert_eq!(data.root(), Path::new("/data/omfp"));
        assert_eq!(data.db(), Path::new("/data/omfp/omfp.db"));
        assert_eq!(data.token(), Path::new("/data/omfp/token"));
        assert_eq!(data.thread("T1"), Path::new("/data/omfp/threads/T1"));
        assert_eq!(
            data.thread_sessions("T1"),
            Path::new("/data/omfp/threads/T1/sessions")
        );
        assert_eq!(
            data.thread_log("T1"),
            Path::new("/data/omfp/threads/T1/omp.log")
        );
        assert_eq!(data.widgets(), Path::new("/data/omfp/widgets"));
        assert_eq!(data.omp_extension(), Path::new("/data/omfp/omp"));
        assert_eq!(
            data.widget_builder(),
            Path::new("/data/omfp/omp/agents/widget-builder.md")
        );
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    // spec: DMN-180
    #[test]
    fn the_data_directory_is_created_private_and_an_open_one_is_tightened() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("share/omfp");

        assert_eq!(create_private_dir(&data).unwrap(), None);
        assert_eq!(mode(&data), 0o700);
        assert_eq!(create_private_dir(&data).unwrap(), None);

        std::fs::set_permissions(&data, Permissions::from_mode(0o755)).unwrap();
        assert_eq!(create_private_dir(&data).unwrap(), Some(0o755));
        assert_eq!(mode(&data), 0o700);

        std::fs::write(dir.path().join("file"), "").unwrap();
        assert!(create_private_dir(&dir.path().join("file")).is_err());
    }

    // spec: DMN-180
    #[test]
    fn logs_are_opened_private_and_an_open_one_is_tightened() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("daemon.log");
        open_private_log(&log).unwrap();
        assert_eq!(mode(&log), 0o600);

        std::fs::set_permissions(&log, Permissions::from_mode(0o644)).unwrap();
        open_private_log(&log).unwrap();
        assert_eq!(mode(&log), 0o600);

        let other = dir.path().join("other");
        std::fs::write(&other, "").unwrap();
        std::fs::set_permissions(&other, Permissions::from_mode(0o640)).unwrap();
        assert_eq!(make_private_file(&other).unwrap(), Some(0o640));
        assert_eq!(make_private_file(&other).unwrap(), None);
        assert_eq!(mode(&other), 0o600);
    }
}
