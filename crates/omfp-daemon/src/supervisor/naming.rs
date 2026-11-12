//! Thread titles (DMN-073): the first message's placeholder, the generated title that replaces it, titles set by a
//! client or the agent, and the omp session named after them.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use omp_rpc::SetSessionNameCommand;
use tracing::warn;

use super::{Actor, Result, Supervisor, SupervisorError, blocking, lock};
use crate::agent::OmpCall;
use crate::store::{Thread, TitleSource};
use crate::titles::{self, TitleRequest};

impl Supervisor {
    /// Gives an unnamed thread the placeholder of its first message and starts generating its title (DMN-073).
    pub(super) async fn name_from_first_message(&self, actor: &Actor, message: String) {
        let placeholder = titles::placeholder(&message);
        if placeholder.is_empty() || actor.titled.load(Ordering::SeqCst) {
            return;
        }
        let threads = Arc::clone(&self.inner.threads);
        let workspaces = Arc::clone(&self.inner.workspaces);
        let id = actor.id.clone();
        let named = blocking(move || {
            let thread = threads.get(&id)?;
            if thread.title_source != TitleSource::Placeholder || !thread.title.is_empty() {
                return Ok::<_, SupervisorError>(None);
            }
            let cwd = workspaces.get(&thread.workspace_id)?.path;
            threads.replace_placeholder(&id, &placeholder, TitleSource::Placeholder)?;
            Ok(Some(cwd))
        })
        .await;
        let cwd = match named {
            Ok(Ok(Some(cwd))) => cwd,
            Ok(Ok(None)) => return,
            Ok(Err(error)) | Err(error) => {
                warn!(thread = %actor.id, %error, "cannot store the placeholder title");
                return;
            }
        };
        if actor.titled.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.inner.changes.send(actor.id.clone());
        if self.inner.config.title_generation {
            self.spawn_title_generation(actor, cwd, message);
        }
    }

    /// Runs omp in print mode for the thread's title; the title replaces the placeholder unless a title was set
    /// meanwhile, and names the omp session (DMN-073).
    fn spawn_title_generation(&self, actor: &Actor, cwd: PathBuf, message: String) {
        let request = TitleRequest {
            program: self.inner.config.omp_program.clone(),
            cwd,
            env: self.inner.config.omp_env.clone(),
            message,
        };
        let supervisor = self.clone();
        let id = actor.id.clone();
        let task = tokio::spawn(async move {
            let launcher = Arc::clone(&supervisor.inner.launcher);
            let title = match blocking(move || launcher.generate_title(&request)).await {
                Ok(Ok(title)) => title,
                Ok(Err(error)) => {
                    warn!(thread = %id, %error, "title generation failed; the placeholder stays");
                    return;
                }
                Err(error) => {
                    warn!(thread = %id, %error, "title generation failed; the placeholder stays");
                    return;
                }
            };
            let threads = Arc::clone(&supervisor.inner.threads);
            let (thread_id, generated) = (id.clone(), title.clone());
            let stored = blocking(move || {
                threads.replace_placeholder(&thread_id, &generated, TitleSource::Generated)
            })
            .await;
            match stored {
                Ok(Ok(true)) => {
                    // Named first, so a client seeing the title on the index finds the session named too.
                    supervisor.name_session(&id, title).await;
                    let _ = supervisor.inner.changes.send(id.clone());
                }
                Ok(Ok(false)) => {}
                Ok(Err(error)) => warn!(thread = %id, %error, "cannot store the generated title"),
                Err(error) => warn!(thread = %id, %error, "cannot store the generated title"),
            }
        });
        *lock(&actor.title_task) = Some(task);
    }

    /// Waits until the thread's title generation, if any, is over.
    #[cfg(test)]
    pub(super) async fn title_generated(&self, id: &str) {
        let task = self
            .lookup(id)
            .and_then(|actor| lock(&actor.title_task).take());
        if let Some(task) = task {
            let _ = task.await;
        }
    }

    /// Sets the thread's title and names its omp session after it (DMN-073). A [`TitleSource::Set`] title is never
    /// replaced by a generated one.
    pub async fn set_title(&self, id: &str, title: String, source: TitleSource) -> Result<Thread> {
        let threads = Arc::clone(&self.inner.threads);
        let (thread_id, stored) = (id.to_owned(), title.clone());
        let thread = blocking(move || threads.rename(&thread_id, &stored, source)).await??;
        self.name_session(id, title).await;
        let _ = self.inner.changes.send(id.to_owned());
        Ok(thread)
    }

    /// Sends `set_session_name` to the thread's omp when it runs (omp rejects an empty name); a stopped thread's
    /// session keeps its name until the next title change.
    pub(super) async fn name_session(&self, id: &str, name: String) {
        let Some(process) = self.lookup(id).and_then(|actor| actor.process()) else {
            return;
        };
        if name.trim().is_empty() {
            return;
        }
        let call = OmpCall::SetSessionName(SetSessionNameCommand { name });
        match blocking(move || process.call(call)).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => warn!(thread = %id, %error, "cannot name the omp session"),
            Err(error) => warn!(thread = %id, %error, "cannot name the omp session"),
        }
    }
}
