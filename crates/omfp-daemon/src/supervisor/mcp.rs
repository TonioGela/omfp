//! The MCP servers of a workspace and a thread (M13): the configured list (DMN-190), the check of each server by the
//! thread's running omp with `/mcp test` (DMN-191), and the reconnect, a restart of the idle process (DMN-192).

use std::path::Path;
use std::time::Duration;

use omfp_core::ThreadEvent;
use omfp_protocol::{McpServer, McpServers};
use omp_rpc::{PromptCommand, RpcNotification};
use tokio::sync::broadcast::error::RecvError;
use tracing::warn;

use super::{Result, Supervisor, SupervisorError, blocking};
use crate::omp_mcp::{self, Check, Homes};

/// How long the check's `command_output` may trail omp's answer to the prompt: omp writes the output first, so it is
/// only in transit through the event pump (DMN-191).
const OUTPUT_WAIT: Duration = Duration::from_secs(10);

impl Supervisor {
    fn homes(&self) -> Homes {
        Homes {
            home: self.inner.config.home.clone(),
            agent_dir: self.inner.config.agent_dir.clone(),
        }
    }

    /// The servers configured for workspace `workspace_id`, unchecked (PRT-110, DMN-190).
    pub async fn workspace_mcp_servers(&self, workspace_id: &str) -> Result<McpServers> {
        let workspaces = std::sync::Arc::clone(&self.inner.workspaces);
        let id = workspace_id.to_owned();
        let path = blocking(move || workspaces.get(&id)).await??.path;
        let servers = omp_mcp::configured(&path, &self.homes());
        Ok(self.listing(workspace_id, None, false, &path, servers))
    }

    /// The servers of the thread's workspace, each checkable one checked by the thread's omp when it runs (PRT-111,
    /// DMN-191). A stopped thread is not started: its servers run only with it.
    pub async fn thread_mcp_servers(&self, thread_id: &str) -> Result<McpServers> {
        self.thread_servers(thread_id, None).await
    }

    /// Reconnects the thread's MCP servers: omp 18.18.6 reconnects none over RPC (`/mcp reconnect` is TUI-only,
    /// `/mcp reload` does not reconnect), so the idle process restarts and connects them all again; then `name`, or
    /// every checkable server, is checked (PRT-112, DMN-192). A working thread is refused: a restart would abort it.
    pub async fn mcp_reconnect(&self, thread_id: &str, name: Option<&str>) -> Result<McpServers> {
        let actor = self.actor(thread_id).await?;
        if !actor.status().settled {
            return Err(SupervisorError::Busy);
        }
        self.restart(thread_id).await?;
        self.thread_servers(thread_id, name).await
    }

    /// The thread's servers; with `only`, the others are listed unchecked.
    async fn thread_servers(&self, thread_id: &str, only: Option<&str>) -> Result<McpServers> {
        let threads = std::sync::Arc::clone(&self.inner.threads);
        let workspaces = std::sync::Arc::clone(&self.inner.workspaces);
        let id = thread_id.to_owned();
        let (workspace_id, path) = blocking(move || {
            let thread = threads.get(&id)?;
            let workspace = workspaces.get(&thread.workspace_id)?;
            Ok::<_, SupervisorError>((thread.workspace_id, workspace.path))
        })
        .await??;
        let configured = omp_mcp::configured(&path, &self.homes());
        let running = self.is_running(thread_id);
        let mut servers = Vec::with_capacity(configured.len());
        for server in configured {
            let check =
                if running && server.checkable && only.is_none_or(|name| name == server.name) {
                    self.mcp_check(thread_id, &server.name).await?
                } else {
                    None
                };
            servers.push(omp_mcp::checked(server, check));
        }
        Ok(self.listing(&workspace_id, Some(thread_id), running, &path, servers))
    }

    fn listing(
        &self,
        workspace_id: &str,
        thread_id: Option<&str>,
        running: bool,
        path: &Path,
        servers: Vec<McpServer>,
    ) -> McpServers {
        McpServers {
            workspace_id: workspace_id.to_owned(),
            thread_id: thread_id.map(str::to_owned),
            running,
            project_config: omp_mcp::project_config(path).to_string_lossy().into_owned(),
            user_config: omp_mcp::user_config(&self.homes())
                .map(|path| path.to_string_lossy().into_owned()),
            servers,
        }
    }

    /// Checks server `name` with the thread's running omp: `/mcp test <name>` runs as a local command (no model turn)
    /// that connects anew, lists the tools and disconnects, and answers with one `command_output` frame (DMN-191).
    /// `None` when the thread does not run or omp sent no answer for this server. Checks of one thread run one at a
    /// time so their outputs cannot cross.
    async fn mcp_check(&self, thread_id: &str, name: &str) -> Result<Option<Check>> {
        let Some(actor) = self.lookup(thread_id) else {
            return Ok(None);
        };
        let _one_at_a_time = actor.local_commands.lock().await;
        let Some(process) = actor.process() else {
            return Ok(None);
        };
        let mut events = actor.log.listen();
        actor.touch();
        let command = PromptCommand {
            message: format!("/mcp test {name}"),
            images: None,
            streaming_behavior: None,
        };
        if let Err(error) = blocking(move || process.prompt(command)).await? {
            warn!(thread = %thread_id, server = %name, %error, "MCP check failed");
            return Ok(Some(Check::Failed(format!(
                "omp did not answer the check: {error}"
            ))));
        }
        let wait = async {
            loop {
                match events.recv().await {
                    Ok(sequenced) => {
                        if let ThreadEvent::Omp {
                            frame: RpcNotification::CommandOutput(output),
                        } = &sequenced.event
                            && let Some(check) = omp_mcp::parse_check(name, &output.text)
                        {
                            return Some(check);
                        }
                    }
                    Err(RecvError::Lagged(_)) => {}
                    Err(RecvError::Closed) => return None,
                }
            }
        };
        Ok(tokio::time::timeout(OUTPUT_WAIT, wait).await.ok().flatten())
    }
}
