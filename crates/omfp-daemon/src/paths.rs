//! Layout of the daemon's data directory (00-overview, "On-disk data").

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
}
