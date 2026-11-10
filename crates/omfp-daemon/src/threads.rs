//! Thread records (02-daemon, "Threads in a workspace"): creation in a workspace with a private session directory,
//! metadata, and the roots last given to the thread's omp session. Process supervision lives elsewhere; this module
//! never touches omp.
//!
//! Calls are blocking; the server layer runs them on blocking tasks.

use std::path::PathBuf;
use std::sync::Arc;

use crate::paths::DataDir;
use crate::store::{Provenance, Store, StoreError, Thread, new_id, now_ms};

#[derive(Debug, thiserror::Error)]
pub enum ThreadError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T, E = ThreadError> = std::result::Result<T, E>;

/// Parameters of [`Threads::create`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewThread {
    pub title: String,
    /// The workspace the thread lives in (ARC-008).
    pub workspace_id: String,
    pub tags: Vec<String>,
    /// `provider/id`.
    pub model: Option<String>,
    pub thinking_level: Option<String>,
    pub approval_mode: Option<String>,
    pub created_by: Provenance,
}

impl NewThread {
    /// A thread created by the user in `workspace_id`, with no tag or setting.
    #[must_use]
    pub fn new(title: impl Into<String>, workspace_id: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            workspace_id: workspace_id.into(),
            tags: Vec::new(),
            model: None,
            thinking_level: None,
            approval_mode: None,
            created_by: Provenance::User,
        }
    }
}

#[derive(Debug)]
pub struct Threads {
    store: Arc<Store>,
    data: DataDir,
}

impl Threads {
    #[must_use]
    pub fn new(store: Arc<Store>, data: DataDir) -> Self {
        Self { store, data }
    }

    /// Creates a thread in a workspace, with its private session directory (ARC-020).
    pub fn create(&self, new: NewThread) -> Result<Thread> {
        self.store.check_provenance(&new.created_by)?;
        self.store.workspace(&new.workspace_id)?;
        let id = new_id();
        let session_dir = std::path::absolute(self.data.thread_sessions(&id))?;
        std::fs::create_dir_all(&session_dir)?;
        let thread = Thread {
            id,
            title: new.title,
            workspace_id: new.workspace_id,
            session_dir,
            omp_session_id: None,
            approval_mode: new.approval_mode,
            model: new.model,
            thinking_level: new.thinking_level,
            created_by: new.created_by,
            created_at: now_ms(),
            archived_at: None,
            tags: new.tags,
        };
        if let Err(error) = self.store.insert_thread(&thread) {
            // Best effort: no orphan directory for a thread that does not exist.
            let _ = std::fs::remove_dir_all(self.data.thread(&thread.id));
            return Err(error.into());
        }
        self.get(&thread.id)
    }

    pub fn get(&self, id: &str) -> Result<Thread> {
        Ok(self.store.thread(id)?)
    }

    /// Every thread, archived ones included, oldest first.
    pub fn list(&self) -> Result<Vec<Thread>> {
        Ok(self.store.threads()?)
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<Thread> {
        self.store.set_thread_title(id, title)?;
        self.get(id)
    }

    /// Replaces the tags (stored sorted, without duplicates).
    pub fn set_tags(&self, id: &str, tags: &[String]) -> Result<Thread> {
        self.store.set_thread_tags(id, tags)?;
        self.get(id)
    }

    pub fn archive(&self, id: &str) -> Result<Thread> {
        self.store.set_thread_archived_at(id, Some(now_ms()))?;
        self.get(id)
    }

    pub fn unarchive(&self, id: &str) -> Result<Thread> {
        self.store.set_thread_archived_at(id, None)?;
        self.get(id)
    }

    /// Records the omp session the thread is bound to (DMN-032, DMN-072).
    pub fn set_omp_session_id(&self, id: &str, session_id: &str) -> Result<()> {
        Ok(self.store.set_thread_omp_session_id(id, Some(session_id))?)
    }

    pub fn set_model(&self, id: &str, model: Option<&str>) -> Result<Thread> {
        self.store.set_thread_model(id, model)?;
        self.get(id)
    }

    pub fn set_thinking_level(&self, id: &str, level: Option<&str>) -> Result<Thread> {
        self.store.set_thread_thinking_level(id, level)?;
        self.get(id)
    }

    /// The roots last given to the thread's omp session (DMN-029).
    pub fn roots(&self, id: &str) -> Result<Vec<PathBuf>> {
        Ok(self.store.thread_roots(id)?)
    }

    pub fn set_roots(&self, id: &str, roots: &[PathBuf]) -> Result<()> {
        Ok(self.store.set_thread_roots(id, roots)?)
    }

    /// Deletes the thread and its directory (sessions, log). Its workspace is never removed implicitly. The caller
    /// makes sure no omp process runs for it.
    pub fn delete(&self, id: &str) -> Result<()> {
        self.store.delete_thread(id)?;
        match std::fs::remove_dir_all(self.data.thread(id)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }
}
