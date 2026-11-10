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

    /// The thread's omp cwd (DMN-028).
    #[must_use]
    pub fn thread_home(&self, thread_id: &str) -> PathBuf {
        self.thread(thread_id).join("home")
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

    /// A daemon-managed worktree.
    #[must_use]
    pub fn worktree(&self, repo_slug: &str, workspace_slug: &str) -> PathBuf {
        self.root
            .join("worktrees")
            .join(repo_slug)
            .join(workspace_slug)
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
            data.thread_home("T1"),
            Path::new("/data/omfp/threads/T1/home")
        );
        assert_eq!(
            data.thread_sessions("T1"),
            Path::new("/data/omfp/threads/T1/sessions")
        );
        assert_eq!(
            data.thread_log("T1"),
            Path::new("/data/omfp/threads/T1/omp.log")
        );
        assert_eq!(
            data.worktree("app", "fix-1"),
            Path::new("/data/omfp/worktrees/app/fix-1")
        );
    }
}
