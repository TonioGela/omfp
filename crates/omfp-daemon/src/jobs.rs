//! Jobs (DMN-080, DMN-081): shell commands the daemon runs for a workspace, each as `sh -c <command>` in its own
//! process group, its output lines published on the `job/<id>` topic (PRT-041). Jobs live in memory and belong to
//! their workspace, not to a client.

use std::collections::{HashMap, VecDeque};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use omfp_protocol::{ErrorBody, ErrorCode, JobEvent, JobInfo, JobLine, JobStatus, JobStream};
use rustix::io::Errno;
use rustix::process::{Pid, Signal, kill_process_group};
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio::sync::watch;
use tracing::warn;

use crate::event_log::EventLog;
use crate::store::{new_id, now_ms};

/// A longer output line is cut into pieces of at most this many bytes.
pub const LINE_BYTES: usize = 64 * 1024;
/// The topic's state keeps the newest lines totalling at most this much text.
pub const KEPT_TEXT_BYTES: usize = 1024 * 1024;
/// Events kept for replay; also how far a subscriber may fall behind (PRT-023).
pub const REPLAY_EVENTS: usize = 4096;
/// Finished jobs kept per workspace; the oldest finished one is forgotten first.
pub const FINISHED_KEPT: usize = 32;
/// How long a job's shell has after SIGTERM before its group gets SIGKILL.
pub const KILL_GRACE: Duration = Duration::from_secs(2);

const READ_CHUNK: usize = 8192;

/// A `job/<id>` topic.
pub type JobLog = EventLog<JobEvent, JobTopic>;

/// Why a job request failed.
#[derive(Debug, thiserror::Error)]
pub enum JobError {
    #[error("job {0} not found")]
    NotFound(String),
    #[error("the command is empty")]
    EmptyCommand,
    #[error("the daemon is shutting down")]
    ShuttingDown,
    #[error("cannot start the job: {0}")]
    Spawn(#[from] std::io::Error),
}

impl From<JobError> for ErrorBody {
    fn from(error: JobError) -> Self {
        let code = match &error {
            JobError::NotFound(_) => ErrorCode::NotFound,
            JobError::EmptyCommand => ErrorCode::BadRequest,
            JobError::ShuttingDown | JobError::Spawn(_) => ErrorCode::Internal,
        };
        ErrorBody::new(code, error.to_string())
    }
}

/// The state of a `job/<id>` topic, serialized as an [`omfp_protocol::JobState`]; it also counts the bytes of its
/// lines so the 1 MiB cap costs nothing per line.
#[derive(Debug, Clone)]
pub struct JobTopic {
    job: JobInfo,
    lines: VecDeque<JobLine>,
    text_bytes: usize,
    dropped: u64,
}

impl JobTopic {
    fn new(job: JobInfo) -> Self {
        Self {
            job,
            lines: VecDeque::new(),
            text_bytes: 0,
            dropped: 0,
        }
    }
}

/// [`omfp_protocol::JobState`]'s wire shape over borrowed parts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JobStateRef<'a> {
    job: &'a JobInfo,
    lines: &'a VecDeque<JobLine>,
    dropped: u64,
}

impl Serialize for JobTopic {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        JobStateRef {
            job: &self.job,
            lines: &self.lines,
            dropped: self.dropped,
        }
        .serialize(serializer)
    }
}

/// The reducer of `job/<id>` (PRT-041).
fn apply(topic: &mut JobTopic, event: &JobEvent) {
    match event {
        JobEvent::JobOutput { stream, text } => {
            topic.text_bytes += text.len();
            topic.lines.push_back(JobLine {
                stream: *stream,
                text: text.clone(),
            });
            while topic.text_bytes > KEPT_TEXT_BYTES
                && let Some(oldest) = topic.lines.pop_front()
            {
                topic.text_bytes -= oldest.text.len();
                topic.dropped += 1;
            }
        }
        JobEvent::JobExited {
            code,
            signal,
            ended_at,
        } => {
            topic.job.status = JobStatus::Exited {
                code: *code,
                signal: *signal,
                ended_at: *ended_at,
            };
        }
    }
}

/// Cuts a byte stream into lines (DMN-080): invalid UTF-8 replaced, the line break and a `\r` before it dropped, a
/// line longer than [`LINE_BYTES`] cut into pieces (never inside a UTF-8 character).
#[derive(Debug, Default)]
struct Lines {
    pending: Vec<u8>,
}

impl Lines {
    fn push(&mut self, mut bytes: &[u8], emit: &mut impl FnMut(String)) {
        while let Some(end) = bytes.iter().position(|&byte| byte == b'\n') {
            self.pending.extend_from_slice(&bytes[..end]);
            bytes = &bytes[end + 1..];
            if self.pending.last() == Some(&b'\r') {
                self.pending.pop();
            }
            self.cut(LINE_BYTES, emit);
            emit(text(&self.pending));
            self.pending.clear();
        }
        self.pending.extend_from_slice(bytes);
        // One byte more stays pending: it may be a `\r` whose `\n` has not arrived yet.
        self.cut(LINE_BYTES + 1, emit);
    }

    /// The last line, without a line break.
    fn finish(mut self, emit: &mut impl FnMut(String)) {
        if !self.pending.is_empty() {
            self.cut(LINE_BYTES, emit);
            emit(text(&self.pending));
        }
    }

    fn cut(&mut self, longest: usize, emit: &mut impl FnMut(String)) {
        while self.pending.len() > longest {
            let end = piece_end(&self.pending);
            emit(text(&self.pending[..end]));
            self.pending.drain(..end);
        }
    }
}

/// Where the first piece of a too long line ends: at [`LINE_BYTES`], or before the UTF-8 character crossing it.
fn piece_end(bytes: &[u8]) -> usize {
    let continuation = |at: usize| bytes[at] & 0xC0 == 0x80;
    let mut end = LINE_BYTES;
    while end > LINE_BYTES - 3 && continuation(end) {
        end -= 1;
    }
    if continuation(end) { LINE_BYTES } else { end }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Publishes the lines of one output until it closes.
async fn read_lines(mut output: impl AsyncRead + Unpin, stream: JobStream, log: &JobLog) {
    let mut lines = Lines::default();
    let mut emit = |text| {
        log.push(JobEvent::JobOutput { stream, text });
    };
    let mut chunk = vec![0; READ_CHUNK];
    loop {
        match output.read(&mut chunk).await {
            Ok(0) => break,
            Ok(read) => lines.push(&chunk[..read], &mut emit),
            Err(error) => {
                warn!(%error, "cannot read a job's output");
                break;
            }
        }
    }
    lines.finish(&mut emit);
}

/// Waits up to `grace` for the shell to exit; otherwise calls `kill` and waits for the exit (DMN-081).
async fn escalate(mut exited: watch::Receiver<bool>, grace: Duration, kill: impl FnOnce()) {
    // The condition's temporaries (the watch's read guard) are gone before the block runs.
    if tokio::time::timeout(grace, exited.wait_for(|exited| *exited))
        .await
        .is_err()
    {
        kill();
        let _ = exited.wait_for(|exited| *exited).await;
    }
}

/// Signals a job's process group.
#[derive(Debug)]
struct Control {
    group: Pid,
    /// Set once the shell exited.
    exited: watch::Sender<bool>,
    /// Set once the job finished (shell exited, outputs closed): it is never signalled again.
    finished: AtomicBool,
}

impl Control {
    fn signal(&self, signal: Signal) {
        match kill_process_group(self.group, signal) {
            // The group may be gone already.
            Err(error) if error != Errno::SRCH => {
                warn!(group = ?self.group, ?signal, %error, "cannot signal a job");
            }
            _ => {}
        }
    }

    /// Sends SIGTERM to the group now; the returned future sends SIGKILL [`KILL_GRACE`] later unless the shell
    /// exited, and ends once it exited (DMN-081). `None` when the job finished: nothing is sent.
    fn terminate(self: &Arc<Self>) -> Option<impl Future<Output = ()> + Send + 'static> {
        if self.finished.load(Ordering::Acquire) {
            return None;
        }
        self.signal(Signal::TERM);
        let control = Arc::clone(self);
        Some(async move {
            let exited = control.exited.subscribe();
            escalate(exited, KILL_GRACE, || control.signal(Signal::KILL)).await;
        })
    }
}

#[derive(Debug)]
struct Entry {
    workspace_id: String,
    log: Arc<JobLog>,
    control: Arc<Control>,
}

#[derive(Debug, Default)]
struct Registry {
    jobs: HashMap<String, Entry>,
    /// The finished jobs of each workspace, oldest first.
    finished: HashMap<String, VecDeque<String>>,
    /// Set at shutdown: no job starts any more.
    closed: bool,
}

/// Every job of the daemon. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct Jobs {
    registry: Arc<Mutex<Registry>>,
}

impl Jobs {
    fn lock(&self) -> MutexGuard<'_, Registry> {
        self.registry.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `sh -c <command>` in `folder` for the workspace (DMN-080): the daemon's environment, stdin from
    /// `/dev/null`, a new process group. Must run inside a Tokio runtime.
    pub fn start(
        &self,
        workspace_id: &str,
        folder: &Path,
        command: &str,
    ) -> Result<JobInfo, JobError> {
        if command.trim().is_empty() {
            return Err(JobError::EmptyCommand);
        }
        let mut registry = self.lock();
        if registry.closed {
            return Err(JobError::ShuttingDown);
        }
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(folder)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .kill_on_drop(true)
            .spawn()?;
        let group = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .and_then(Pid::from_raw)
            .ok_or_else(|| std::io::Error::other("the shell has no process id"))?;
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            return Err(std::io::Error::other("the shell's outputs are not piped").into());
        };
        let job = JobInfo {
            id: new_id(),
            workspace_id: workspace_id.to_owned(),
            command: command.to_owned(),
            started_at: now_ms(),
            status: JobStatus::Running,
        };
        let log = Arc::new(EventLog::new(
            new_id(),
            JobTopic::new(job.clone()),
            apply,
            REPLAY_EVENTS,
            REPLAY_EVENTS,
        ));
        let control = Arc::new(Control {
            group,
            exited: watch::Sender::new(false),
            finished: AtomicBool::new(false),
        });
        registry.jobs.insert(
            job.id.clone(),
            Entry {
                workspace_id: job.workspace_id.clone(),
                log: Arc::clone(&log),
                control: Arc::clone(&control),
            },
        );
        drop(registry);
        tokio::spawn(
            self.clone()
                .run(job.id.clone(), child, (stdout, stderr), log, control),
        );
        Ok(job)
    }

    /// Follows the job until it finished: shell exited and both outputs closed.
    async fn run(
        self,
        id: String,
        mut child: Child,
        (stdout, stderr): (ChildStdout, ChildStderr),
        log: Arc<JobLog>,
        control: Arc<Control>,
    ) {
        let shell = async {
            let status = child.wait().await;
            control.exited.send_replace(true);
            status
        };
        let (status, (), ()) = tokio::join!(
            shell,
            read_lines(stdout, JobStream::Stdout, &log),
            read_lines(stderr, JobStream::Stderr, &log),
        );
        let (code, signal) = match status {
            Ok(status) => (status.code(), status.signal()),
            Err(error) => {
                warn!(job = %id, %error, "cannot wait for a job's shell");
                (None, None)
            }
        };
        control.finished.store(true, Ordering::Release);
        // Kept as finished before anyone sees it finish.
        self.finish(&id);
        log.push(JobEvent::JobExited {
            code,
            signal,
            ended_at: now_ms(),
        });
    }

    /// Keeps `id` among its workspace's finished jobs, forgetting the oldest beyond [`FINISHED_KEPT`].
    fn finish(&self, id: &str) {
        let mut guard = self.lock();
        let registry = &mut *guard;
        // A job forgotten with its workspace stays forgotten.
        let Some(entry) = registry.jobs.get(id) else {
            return;
        };
        let finished = registry
            .finished
            .entry(entry.workspace_id.clone())
            .or_default();
        finished.push_back(id.to_owned());
        if finished.len() > FINISHED_KEPT
            && let Some(oldest) = finished.pop_front()
        {
            registry.jobs.remove(&oldest);
        }
    }

    /// The log of `job/<id>`.
    pub fn log(&self, id: &str) -> Result<Arc<JobLog>, JobError> {
        self.lock()
            .jobs
            .get(id)
            .map(|entry| Arc::clone(&entry.log))
            .ok_or_else(|| JobError::NotFound(id.to_owned()))
    }

    /// SIGTERM to the job's group, SIGKILL [`KILL_GRACE`] later if the shell has not exited (DMN-081); a finished
    /// job is left alone.
    pub fn kill(&self, id: &str) -> Result<(), JobError> {
        let control = self
            .lock()
            .jobs
            .get(id)
            .map(|entry| Arc::clone(&entry.control))
            .ok_or_else(|| JobError::NotFound(id.to_owned()))?;
        if let Some(escalation) = control.terminate() {
            tokio::spawn(escalation);
        }
        Ok(())
    }

    /// Kills the running jobs of a workspace (DMN-081) and returns once their shells exited.
    pub async fn kill_workspace(&self, workspace_id: &str) {
        let escalations: Vec<_> = self
            .lock()
            .jobs
            .values()
            .filter(|entry| entry.workspace_id == workspace_id)
            .filter_map(|entry| entry.control.terminate())
            .collect();
        futures::future::join_all(escalations).await;
    }

    /// Forgets the jobs of a removed workspace; one that started meanwhile is killed.
    pub fn forget_workspace(&self, workspace_id: &str) {
        let mut registry = self.lock();
        registry.finished.remove(workspace_id);
        for (_, entry) in registry
            .jobs
            .extract_if(|_, entry| entry.workspace_id == workspace_id)
        {
            if let Some(escalation) = entry.control.terminate() {
                tokio::spawn(escalation);
            }
        }
    }

    /// Kills every running job (DMN-081) and returns once their shells exited; no job starts afterwards.
    pub async fn shutdown(&self) {
        let escalations: Vec<_> = {
            let mut registry = self.lock();
            registry.closed = true;
            registry
                .jobs
                .values()
                .filter_map(|entry| entry.control.terminate())
                .collect()
        };
        futures::future::join_all(escalations).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use omfp_protocol::JobState;

    use super::*;

    fn lines(chunks: &[&[u8]]) -> Vec<String> {
        let mut out = Vec::new();
        let mut emit = |text| out.push(text);
        let mut lines = Lines::default();
        for chunk in chunks {
            lines.push(chunk, &mut emit);
        }
        lines.finish(&mut emit);
        out
    }

    // spec: DMN-080
    #[test]
    fn output_is_cut_at_line_breaks_without_a_carriage_return_before_them() {
        assert_eq!(
            lines(&[b"one\ntw", b"o\r", b"\nthree\r\n\nmid\rdle\nlast"]),
            ["one", "two", "three", "", "mid\rdle", "last"]
        );
    }

    // spec: DMN-080
    #[test]
    fn invalid_utf8_is_replaced() {
        assert_eq!(lines(&[b"bad \xff byte\n"]), ["bad \u{FFFD} byte"]);
    }

    // spec: DMN-080
    #[test]
    fn a_line_longer_than_64_kib_is_cut_into_pieces() {
        let long = vec![b'a'; 2 * LINE_BYTES + 10];
        let exact = vec![b'b'; LINE_BYTES];
        let pieces = lines(&[&long[..1000], &long[1000..], b"\n", &exact, b"\r\n"]);
        let lengths: Vec<usize> = pieces.iter().map(String::len).collect();
        assert_eq!(lengths, [LINE_BYTES, LINE_BYTES, 10, LINE_BYTES]);
        assert_eq!(pieces[3], "b".repeat(LINE_BYTES));
    }

    // spec: DMN-080
    #[test]
    fn a_cut_never_splits_a_utf8_character() {
        let mut line = vec![b'a'; LINE_BYTES - 1];
        line.extend_from_slice("é".as_bytes());
        line.extend_from_slice(b"z\n");
        let pieces = lines(&[&line]);
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].len(), LINE_BYTES - 1);
        assert_eq!(pieces[1], "éz");
    }

    fn job() -> JobInfo {
        JobInfo {
            id: "j1".into(),
            workspace_id: "w1".into(),
            command: "true".into(),
            started_at: 1,
            status: JobStatus::Running,
        }
    }

    fn output(stream: JobStream, text: &str) -> JobEvent {
        JobEvent::JobOutput {
            stream,
            text: text.into(),
        }
    }

    // spec: DMN-080, PRT-041
    #[test]
    fn the_state_keeps_the_newest_mebibyte_of_lines_and_counts_the_dropped_ones() {
        let mut topic = JobTopic::new(job());
        let line = "x".repeat(1024);
        for _ in 0..1024 {
            apply(&mut topic, &output(JobStream::Stdout, &line));
        }
        assert_eq!((topic.lines.len(), topic.dropped), (1024, 0));

        apply(&mut topic, &output(JobStream::Stderr, "y"));
        apply(&mut topic, &output(JobStream::Stdout, "z"));

        let state = JobState {
            job: topic.job.clone(),
            lines: topic.lines.iter().cloned().collect(),
            dropped: topic.dropped,
        };
        assert_eq!(state.dropped, 1);
        assert_eq!(state.lines.len(), 1025);
        assert_eq!(
            state.lines[1023..],
            [
                JobLine {
                    stream: JobStream::Stderr,
                    text: "y".into()
                },
                JobLine {
                    stream: JobStream::Stdout,
                    text: "z".into()
                }
            ]
        );
        assert_eq!(
            serde_json::to_value(&topic).unwrap(),
            serde_json::to_value(&state).unwrap()
        );
    }

    // spec: DMN-080, PRT-041
    #[test]
    fn job_exited_sets_the_status() {
        let mut topic = JobTopic::new(job());
        apply(
            &mut topic,
            &JobEvent::JobExited {
                code: None,
                signal: Some(9),
                ended_at: 7,
            },
        );
        assert_eq!(
            topic.job.status,
            JobStatus::Exited {
                code: None,
                signal: Some(9),
                ended_at: 7
            }
        );
    }

    // spec: DMN-081
    #[tokio::test(start_paused = true)]
    async fn a_shell_still_running_after_the_grace_period_is_killed() {
        let (exited, receiver) = watch::channel(false);
        let kills = AtomicUsize::new(0);
        let started = tokio::time::Instant::now();

        escalate(receiver, KILL_GRACE, || {
            kills.fetch_add(1, Ordering::SeqCst);
            exited.send_replace(true);
        })
        .await;

        assert_eq!(kills.load(Ordering::SeqCst), 1);
        assert!(started.elapsed() >= KILL_GRACE);
    }

    // spec: DMN-081
    #[tokio::test(start_paused = true)]
    async fn a_shell_exiting_within_the_grace_period_is_not_killed() {
        let (exited, receiver) = watch::channel(false);
        let kills = AtomicUsize::new(0);
        let exit = async {
            tokio::time::sleep(KILL_GRACE / 2).await;
            exited.send_replace(true);
        };

        tokio::join!(
            escalate(receiver, KILL_GRACE, || {
                kills.fetch_add(1, Ordering::SeqCst);
            }),
            exit
        );

        assert_eq!(kills.load(Ordering::SeqCst), 0);
    }

    const WAIT: Duration = Duration::from_secs(60);

    /// The job's state once it finished.
    async fn finished(log: &JobLog) -> JobTopic {
        let mut live = log.subscribe(None).live;
        tokio::time::timeout(WAIT, async {
            while log.with_state(|topic| topic.job.status == JobStatus::Running) {
                let _ = live.recv().await;
            }
        })
        .await
        .expect("the job did not finish");
        log.with_state(Clone::clone)
    }

    // spec: DMN-081
    #[tokio::test(flavor = "multi_thread")]
    async fn at_most_32_finished_jobs_are_kept_per_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let gate = dir.path().join("gate");
        let jobs = Jobs::default();
        let running = jobs
            .start(
                "w1",
                dir.path(),
                &format!("while [ ! -e '{}' ]; do sleep 0.01; done", gate.display()),
            )
            .unwrap();
        let other = jobs.start("w2", dir.path(), "true").unwrap();
        finished(&jobs.log(&other.id).unwrap()).await;
        let mut ids = Vec::new();
        for _ in 0..=FINISHED_KEPT {
            let job = jobs.start("w1", dir.path(), "true").unwrap();
            finished(&jobs.log(&job.id).unwrap()).await;
            ids.push(job.id);
        }

        assert!(matches!(jobs.log(&ids[0]), Err(JobError::NotFound(_))));
        assert!(matches!(jobs.kill(&ids[0]), Err(JobError::NotFound(_))));
        for id in ids[1..].iter().chain([&running.id, &other.id]) {
            assert!(jobs.log(id).is_ok(), "{id} was forgotten");
        }
        std::fs::write(&gate, "").unwrap();
        finished(&jobs.log(&running.id).unwrap()).await;
    }

    // spec: DMN-081
    #[tokio::test(flavor = "multi_thread")]
    async fn no_job_starts_after_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        let jobs = Jobs::default();
        jobs.shutdown().await;
        assert!(matches!(
            jobs.start("w1", dir.path(), "true"),
            Err(JobError::ShuttingDown)
        ));
    }
}
