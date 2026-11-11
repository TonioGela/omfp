//! The RPC host tool adapter (DMN-151): every registry tool (AGT-040) as an essential host tool of a thread, acting
//! as that thread's agent principal (DMN-150).

use std::time::Duration;

use omp_rpc::{HostTool, HostToolOutput};
use tokio::runtime::Handle;

use super::{AgentTool, ToolContext, ToolFuture, registry};
use crate::host_tools::AgentHost;
use crate::server::{Principal, Services};

/// How often a running tool looks at the cancellation flag (DMN-151).
const CANCEL_POLL: Duration = Duration::from_millis(100);

/// The client agents' calls record as provenance (PRT-033).
pub const CLIENT: &str = "omp";

/// The server's [`AgentHost`]: runs the registry's tools on the daemon's runtime from the `omp-rpc` handler thread
/// a host tool runs on.
pub(crate) struct ServicesAgentHost {
    services: Services,
    runtime: Handle,
}

impl ServicesAgentHost {
    /// Must be called on the daemon's runtime.
    pub(crate) fn new(services: Services) -> Self {
        Self {
            services,
            runtime: Handle::current(),
        }
    }
}

impl AgentHost for ServicesAgentHost {
    fn tools(&self, thread_id: &str, workspace_id: &str) -> Vec<HostTool> {
        let context = ToolContext {
            services: self.services.clone(),
            principal: Principal::Agent {
                thread_id: thread_id.to_owned(),
                client: CLIENT.to_owned(),
            },
            workspace_id: Some(workspace_id.to_owned()),
        };
        registry()
            .iter()
            .map(|tool| host_tool(tool, context.clone(), self.runtime.clone()))
            .collect()
    }
}

/// `tool` as a host tool running with `context` on `runtime`: the handler thread blocks on the body, which is dropped
/// once omp cancels the call.
fn host_tool(tool: &AgentTool, context: ToolContext, runtime: Handle) -> HostTool {
    let parameters = tool.parameters.as_object().cloned().unwrap_or_default();
    let run = tool.run;
    HostTool::new(
        tool.name,
        tool.description,
        parameters,
        move |arguments, call| {
            let body = run(context.clone(), arguments);
            runtime
                .block_on(until_cancelled(body, || call.is_cancelled()))
                .map(HostToolOutput::Text)
                .map_err(Into::into)
        },
    )
    .label(tool.label)
}

/// The body's answer, or `cancelled` once `is_cancelled` says omp cancelled the call (DMN-061, DMN-151).
async fn until_cancelled(
    body: ToolFuture,
    is_cancelled: impl Fn() -> bool,
) -> Result<String, String> {
    let cancelled = async {
        let mut tick = tokio::time::interval(CANCEL_POLL);
        loop {
            tick.tick().await;
            if is_cancelled() {
                return;
            }
        }
    };
    tokio::select! {
        answer = body => answer,
        () = cancelled => Err("cancelled".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use omp_rpc::ToolLoadMode;

    use super::super::tests::Harness;
    use super::super::{CALL, THREAD_WAIT};
    use super::*;
    use crate::host_tools::thread_tools;

    // spec: DMN-151, DMN-106, AGT-040
    #[tokio::test]
    async fn every_thread_gets_the_registry_as_essential_host_tools() {
        let harness = Harness::new().await;
        let host = ServicesAgentHost::new(harness.services.clone());

        let tools = thread_tools(
            Arc::new(|_, _| Ok(())),
            Arc::new(|_, _| Ok(())),
            Arc::new(|_| Ok(())),
            &harness.workspace,
            None,
            host.tools(&harness.thread, &harness.workspace),
        );
        let registry = registry();
        for tool in &registry {
            let registered = tools
                .iter()
                .map(HostTool::definition)
                .find(|definition| definition.name == tool.name)
                .unwrap_or_else(|| panic!("{} is not registered", tool.name));
            assert_eq!(registered.load_mode, Some(ToolLoadMode::Essential));
            assert_eq!(registered.description, tool.description);
            assert_eq!(registered.label.as_deref(), Some(tool.label));
            assert_eq!(
                serde_json::Value::Object(registered.parameters.clone()),
                tool.parameters
            );
        }
        let names: Vec<&str> = registry.iter().map(|tool| tool.name).collect();
        assert!(names.contains(&CALL) && names.contains(&THREAD_WAIT));
    }

    // spec: DMN-151, DMN-061
    #[tokio::test(start_paused = true)]
    async fn a_cancelled_call_drops_its_body_within_the_poll() {
        let cancelled = AtomicBool::new(false);
        let forever: ToolFuture = Box::pin(std::future::pending());
        let waiting = until_cancelled(forever, || cancelled.load(Ordering::SeqCst));
        tokio::pin!(waiting);
        tokio::select! {
            biased;
            _ = &mut waiting => panic!("not cancelled yet"),
            () = tokio::time::sleep(Duration::from_millis(250)) => {}
        }
        cancelled.store(true, Ordering::SeqCst);
        let started = tokio::time::Instant::now();
        assert_eq!(waiting.await, Err("cancelled".to_owned()));
        assert!(started.elapsed() <= CANCEL_POLL);

        let done: ToolFuture = Box::pin(async { Ok("done".to_owned()) });
        assert_eq!(until_cancelled(done, || false).await, Ok("done".to_owned()));
    }
}
