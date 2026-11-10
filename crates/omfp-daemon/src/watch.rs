//! Files on the daemon's host the daemon follows by polling (no file-watching dependency): the theme file (DMN-075)
//! and the config file (DMN-009).

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

/// How often a followed file is checked.
pub const POLL: Duration = Duration::from_secs(1);

/// What a followed file held at the last check.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seen {
    Unread,
    Missing,
    Content(String),
}

/// A file checked by polling.
#[derive(Debug)]
pub struct WatchedFile {
    path: PathBuf,
    seen: Seen,
}

impl WatchedFile {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            seen: Seen::Unread,
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the file and reports a change: at the first check, its content when it exists (a missing file is no
    /// change); later, its new content, or `Some(None)` once it is removed. `None` when nothing changed or the file
    /// cannot be read.
    pub fn poll(&mut self) -> Option<Option<String>> {
        let now = match std::fs::read(&self.path) {
            Ok(bytes) => Seen::Content(String::from_utf8_lossy(&bytes).into_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Seen::Missing,
            Err(error) => {
                warn!(path = %self.path.display(), %error, "cannot read a followed file");
                return None;
            }
        };
        if now == self.seen {
            return None;
        }
        let first = self.seen == Seen::Unread;
        self.seen = now.clone();
        match now {
            Seen::Content(content) => Some(Some(content)),
            Seen::Missing => (!first).then_some(None),
            Seen::Unread => None,
        }
    }
}

/// Checks `file` every `period` (tokio's clock, so tests run it in paused time) and sends each change.
#[must_use]
pub fn watch(
    mut file: WatchedFile,
    period: Duration,
    changes: mpsc::UnboundedSender<Option<String>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(period);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let Ok((polled, change)) = tokio::task::spawn_blocking(move || {
                let change = file.poll();
                (file, change)
            })
            .await
            else {
                break;
            };
            file = polled;
            if let Some(change) = change
                && changes.send(change).is_err()
            {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: DMN-075, DMN-009
    #[test]
    fn a_poll_reports_the_file_at_startup_then_only_its_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.css");
        std::fs::write(&path, ":root{}").unwrap();
        let mut file = WatchedFile::new(&path);

        assert_eq!(file.poll(), Some(Some(":root{}".into())));
        assert_eq!(file.poll(), None);
        std::fs::write(&path, ":root{--x:1}").unwrap();
        assert_eq!(file.poll(), Some(Some(":root{--x:1}".into())));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(file.poll(), Some(None));
        assert_eq!(file.poll(), None);
    }

    // spec: DMN-075, DMN-009
    #[test]
    fn a_missing_file_at_startup_is_no_change() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = WatchedFile::new(dir.path().join("config.toml"));
        assert_eq!(file.poll(), None);
        std::fs::write(file.path(), "a = 1").unwrap();
        assert_eq!(file.poll(), Some(Some("a = 1".into())));
    }

    // spec: DMN-075, DMN-009
    #[tokio::test(start_paused = true)]
    async fn the_watcher_sends_each_change_of_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.css");
        let mut file = WatchedFile::new(&path);
        assert_eq!(file.poll(), None);
        let (sender, mut changes) = mpsc::unbounded_channel();
        let watcher = watch(file, POLL, sender);

        std::fs::write(&path, "one").unwrap();
        assert_eq!(changes.recv().await, Some(Some("one".into())));
        // Rewriting the same content is no change: the next one reported is the new content.
        std::fs::write(&path, "one").unwrap();
        tokio::time::advance(POLL * 3).await;
        std::fs::write(&path, "two").unwrap();
        assert_eq!(changes.recv().await, Some(Some("two".into())));
        std::fs::remove_file(&path).unwrap();
        assert_eq!(changes.recv().await, Some(None));
        watcher.abort();
    }
}
