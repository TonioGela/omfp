//! The agent tool registry (AGT-040): every M10 agent tool defined once — name, label, description, argument schema
//! and an async body over the daemon's services — for the adapters that hand them to agents: the RPC host tools of
//! every thread ([`rpc`], DMN-151) and the MCP server.

pub mod catalog;
pub(crate) mod mounts;
mod omfp_call;
pub(crate) mod rpc;
mod wait;

use std::pin::Pin;

use omfp_protocol::{Call, ErrorBody};
use serde_json::{Map, Value};

use crate::server::dispatch::dispatch;
use crate::server::{Principal, Services, authorize};

/// Name of the tool that calls any protocol method (AGT-041).
pub const CALL: &str = "omfp_call";

/// Name of the tool that waits for a thread (AGT-043).
pub const THREAD_WAIT: &str = "omfp_thread_wait";

/// The methods that need a window's connection: `omfp_call` refuses them and the catalogue leaves them out (AGT-041,
/// AGT-042).
pub const CONNECTION_ONLY: &[&str] = &[
    "system.hello",
    "subscribe",
    "unsubscribe",
    "widgets.report",
    "thread.live_connect",
    "thread.live_start",
];

/// What a tool runs with: the daemon's services, the principal it acts as (DMN-150) and the calling thread's
/// workspace (`None` when no thread calls, e.g. MCP with a user token).
#[derive(Clone)]
pub(crate) struct ToolContext {
    pub services: Services,
    pub principal: Principal,
    pub workspace_id: Option<String>,
}

impl ToolContext {
    /// The calling thread, when the principal is a thread's agent.
    pub fn thread_id(&self) -> Option<&str> {
        match &self.principal {
            Principal::Agent { thread_id, .. } => Some(thread_id),
            Principal::Owner { .. } | Principal::Mcp { .. } => None,
        }
    }
}

/// A running tool body: its text answer, or the text of a tool error. Cancelling a call drops the future.
pub(crate) type ToolFuture = Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

/// One tool of the registry (AGT-040).
#[derive(Clone)]
pub(crate) struct AgentTool {
    pub name: &'static str,
    /// Short name shown on the tool's card.
    pub label: &'static str,
    /// What the model reads about the tool.
    pub description: &'static str,
    /// JSON Schema of the arguments object.
    pub parameters: Value,
    pub run: fn(ToolContext, Map<String, Value>) -> ToolFuture,
}

/// Every tool of the registry, in the order agents see them.
pub(crate) fn registry() -> Vec<AgentTool> {
    let mut tools = vec![omfp_call::tool(), wait::tool()];
    tools.extend(mounts::tools());
    tools
}

/// Runs a protocol method as `context`'s principal: the single authorization check (PRT-034), then the handlers a
/// window's request reaches (AGT-040).
pub(crate) async fn call(context: &ToolContext, call: Call) -> Result<Value, ErrorBody> {
    authorize(&context.principal, call.method())?;
    dispatch(&context.services, &context.principal, call).await
}

/// A failed method as a tool error (AGT-041): `<code>: <message>`, omp's code and the details when there are some.
pub(crate) fn error_text(error: &ErrorBody) -> String {
    let code = serde_json::to_value(error.code)
        .ok()
        .and_then(|code| code.as_str().map(str::to_owned))
        .unwrap_or_else(|| format!("{:?}", error.code));
    let omp_code = error
        .omp_code
        .as_ref()
        .map(|omp_code| format!(" (omp code {omp_code})"))
        .unwrap_or_default();
    let details = error
        .details
        .as_ref()
        .map(|details| format!("\ndetails: {details}"))
        .unwrap_or_default();
    format!("{code}: {}{omp_code}{details}", error.message)
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::Arc;

    use serde_json::{Map, Value};
    use tempfile::TempDir;

    use super::*;
    use crate::agent::FakeLauncher;
    use crate::git::{FakeGit, Git};
    use crate::paths::DataDir;
    use crate::store::Store;
    use crate::supervisor::{Supervisor, SupervisorConfig};
    use crate::threads::{NewThread, Threads};
    use crate::widgets::Widgets;
    use crate::workspaces::{NewWorkspace, Workspaces};

    /// Services on the fake launcher with one workspace and one thread in it, the caller.
    pub struct Harness {
        _dir: TempDir,
        pub services: Services,
        pub launcher: FakeLauncher,
        pub workspace: String,
        pub thread: String,
    }

    impl Harness {
        pub async fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let data = DataDir::new(dir.path().join("data"));
            std::fs::create_dir_all(data.root()).unwrap();
            let root = dir.path().join("workspaces");
            let store = Arc::new(Store::open(&data.db(), &root).unwrap());
            let threads = Arc::new(Threads::new(Arc::clone(&store), data.clone()));
            let workspaces = Arc::new(Workspaces::new(
                Arc::clone(&store),
                Arc::new(FakeGit::new()) as Arc<dyn Git>,
                root,
            ));
            let widgets = Arc::new(Widgets::new(data.widgets(), None));
            let launcher = FakeLauncher::new();
            let supervisor = Supervisor::new(
                Arc::new(launcher.clone()),
                Arc::clone(&threads),
                Arc::clone(&workspaces),
                data,
                SupervisorConfig::new("omp"),
            );
            let services = Services::new(store, threads, workspaces, supervisor, widgets)
                .await
                .unwrap();
            let workspace = services
                .workspaces
                .create(NewWorkspace::new())
                .unwrap()
                .workspace
                .id;
            let thread = services
                .threads
                .create(NewThread::new("caller", workspace.clone()))
                .unwrap()
                .id;
            Self {
                _dir: dir,
                services,
                launcher,
                workspace,
                thread,
            }
        }

        /// The context of the caller thread's host tools.
        pub fn context(&self) -> ToolContext {
            ToolContext {
                services: self.services.clone(),
                principal: Principal::Agent {
                    thread_id: self.thread.clone(),
                    client: "omp".to_owned(),
                },
                workspace_id: Some(self.workspace.clone()),
            }
        }
    }

    pub fn args(value: Value) -> Map<String, Value> {
        match value {
            Value::Object(map) => map,
            other => panic!("not an object: {other}"),
        }
    }

    // spec: AGT-040
    #[test]
    fn the_registry_starts_with_omfp_call_and_omfp_thread_wait_and_every_tool_has_an_object_schema()
    {
        let tools = registry();
        let names: Vec<&str> = tools.iter().map(|tool| tool.name).collect();
        assert_eq!(names[..2], [CALL, THREAD_WAIT]);
        for tool in &tools {
            assert_eq!(tool.parameters["type"], "object", "{}", tool.name);
            assert!(!tool.description.is_empty(), "{}", tool.name);
            assert!(!tool.label.is_empty(), "{}", tool.name);
        }
    }
}
