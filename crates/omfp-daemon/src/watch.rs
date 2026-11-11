//! Files on the daemon's host the daemon follows by polling (no file-watching dependency): the theme file (DMN-075)
//! and the config file (DMN-009). A change is reported only once the file settled (DMN-091): two consecutive reads
//! agree on its size, modification time and content, so a writer that is not atomic is never read halfway.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::warn;

/// How often a followed file is checked.
pub const POLL: Duration = Duration::from_secs(1);

/// One read of a followed file.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Snapshot {
    Missing,
    File {
        size: u64,
        modified: Option<SystemTime>,
        content: Vec<u8>,
    },
}

/// What the last reported change held.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Reported {
    Nothing,
    Missing,
    Content(Vec<u8>),
}

/// A file checked by polling.
#[derive(Debug)]
pub struct WatchedFile {
    path: PathBuf,
    /// The previous read, `None` before the first one.
    previous: Option<Snapshot>,
    reported: Reported,
}

impl WatchedFile {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            previous: None,
            reported: Reported::Nothing,
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the file and reports a change once it settled, i.e. this read agrees with the previous one (DMN-091):
    /// the first settled state is reported when the file exists (a missing file is no change); later, its new
    /// content, or `Some(None)` once it is removed. `None` when nothing changed, the file is still changing (or was
    /// never read before) or it cannot be read.
    pub fn poll(&mut self) -> Option<Option<String>> {
        let now = match self.read() {
            Ok(now) => now,
            Err(error) => {
                warn!(path = %self.path.display(), %error, "cannot read a followed file");
                return None;
            }
        };
        if self.previous.as_ref() != Some(&now) {
            self.previous = Some(now);
            return None;
        }
        let settled = match now {
            Snapshot::Missing => Reported::Missing,
            Snapshot::File { content, .. } => Reported::Content(content),
        };
        if settled == self.reported {
            return None;
        }
        let first = self.reported == Reported::Nothing;
        self.reported = settled;
        match &self.reported {
            Reported::Content(content) => Some(Some(String::from_utf8_lossy(content).into_owned())),
            Reported::Missing => (!first).then_some(None),
            Reported::Nothing => None,
        }
    }

    fn read(&self) -> std::io::Result<Snapshot> {
        let mut file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Snapshot::Missing);
            }
            Err(error) => return Err(error),
        };
        let metadata = file.metadata()?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)?;
        Ok(Snapshot::File {
            size: metadata.len(),
            modified: metadata.modified().ok(),
            content,
        })
    }
}

/// Reads `file` twice in a row (startup) and reports it only when both reads agree (DMN-091); otherwise the
/// watcher reports it once it settled.
pub fn first_read(file: &mut WatchedFile) -> Option<Option<String>> {
    file.poll();
    file.poll()
}

/// Checks `file` every `period` (tokio's clock, so tests run it in paused time), starting one period from now, and
/// sends each change.
#[must_use]
pub fn watch(
    mut file: WatchedFile,
    period: Duration,
    changes: mpsc::UnboundedSender<Option<String>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
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

    // spec: DMN-075, DMN-009, DMN-091
    #[test]
    fn a_poll_reports_the_file_at_startup_then_only_its_settled_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.css");
        std::fs::write(&path, ":root{}").unwrap();
        let mut file = WatchedFile::new(&path);

        assert_eq!(first_read(&mut file), Some(Some(":root{}".into())));
        assert_eq!(file.poll(), None);
        std::fs::write(&path, ":root{--x:1}").unwrap();
        assert_eq!(file.poll(), None, "not settled yet");
        assert_eq!(file.poll(), Some(Some(":root{--x:1}".into())));
        assert_eq!(file.poll(), None);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(file.poll(), None, "not settled yet");
        assert_eq!(file.poll(), Some(None));
        assert_eq!(file.poll(), None);
    }

    // spec: DMN-075, DMN-009, DMN-091
    #[test]
    fn a_missing_file_at_startup_is_no_change() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = WatchedFile::new(dir.path().join("config.toml"));
        assert_eq!(first_read(&mut file), None);
        std::fs::write(file.path(), "a = 1").unwrap();
        assert_eq!(file.poll(), None);
        assert_eq!(file.poll(), Some(Some("a = 1".into())));
    }

    // spec: DMN-091
    #[test]
    fn rewriting_the_same_content_is_no_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.css");
        std::fs::write(&path, "one").unwrap();
        let mut file = WatchedFile::new(&path);
        assert_eq!(first_read(&mut file), Some(Some("one".into())));

        std::fs::write(&path, "one").unwrap();
        assert_eq!(file.poll(), None);
        assert_eq!(file.poll(), None);
        assert_eq!(file.poll(), None);
    }

    // spec: DMN-091
    #[test]
    fn a_file_written_in_two_steps_is_reported_once_complete_never_halfway() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.css");
        std::fs::write(&path, "old").unwrap();
        let mut file = WatchedFile::new(&path);
        assert_eq!(first_read(&mut file), Some(Some("old".into())));

        // A writer that is not atomic: it truncates, a poll runs, then it writes.
        std::fs::File::create(&path).unwrap();
        assert_eq!(file.poll(), None, "the truncated file is not settled");
        std::fs::write(&path, "new content").unwrap();
        assert_eq!(file.poll(), None, "the full file is not settled yet");
        assert_eq!(file.poll(), Some(Some("new content".into())));
        assert_eq!(file.poll(), None);
    }

    // spec: DMN-091
    #[test]
    fn a_file_caught_halfway_at_startup_is_reported_by_the_next_poll_that_sees_it_settled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "").unwrap();
        let mut file = WatchedFile::new(&path);

        // The writer truncated; it writes between the two startup reads.
        assert_eq!(file.poll(), None);
        std::fs::write(&path, "a = 1").unwrap();
        assert_eq!(
            file.poll(),
            None,
            "the startup reads disagree: nothing applies"
        );
        assert_eq!(file.poll(), Some(Some("a = 1".into())));
    }

    // spec: DMN-075, DMN-009, DMN-091
    #[tokio::test(start_paused = true)]
    async fn the_watcher_sends_each_change_of_the_file_one_poll_after_it_settled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("theme.css");
        let mut file = WatchedFile::new(&path);
        assert_eq!(first_read(&mut file), None);
        let (sender, mut changes) = mpsc::unbounded_channel();
        let start = tokio::time::Instant::now();
        let watcher = watch(file, POLL, sender);

        std::fs::write(&path, "one").unwrap();
        assert_eq!(changes.recv().await, Some(Some("one".into())));
        // The first poll sees the new file, the next one finds it settled.
        assert_eq!(start.elapsed(), POLL * 2);
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
