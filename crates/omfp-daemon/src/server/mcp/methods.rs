//! The MCP server's methods (AGT-050, AGT-052, DMN-156, DMN-157): the identity and capabilities, the agent tool
//! registry as tools, the method catalogue as resources.

use axum::http::StatusCode;
use omp_rpc::HostUriResultContentType;
use serde_json::{Map, Value, json};

use super::{Era, LEGACY, RpcError, UNNAMED_CLIENT, code, supported};
use crate::agent_tools::catalog::{self, METHODS_URL};
use crate::agent_tools::{ToolContext, ToolFuture, registry};
use crate::server::{Principal, Services};

/// How long the lists, `server/discover` and `resources/read` stay fresh: they change only with the daemon (DMN-156).
const TTL_MS: u64 = 3_600_000;

/// The URI template of one method's schemas (AGT-052).
const METHOD_TEMPLATE: &str = "omfp://protocol/methods/{name}";

/// What the server tells the model about itself.
const INSTRUCTIONS: &str = "omfp runs omp agent threads in workspace folders and shows them in its UI. `omfp_call` \
    calls any omfp protocol method (the resource omfp://protocol/methods lists them, \
    omfp://protocol/methods/<name> gives one's params and result schemas), e.g. `threads.list`, `threads.create`, \
    `thread.prompt`; `omfp_thread_wait` waits for a thread instead of polling it; the UI mount and component tools \
    put widgets in front of the user and need `workspaceId` (from `workspaces.list`).";

/// A method's answer: a result, or a tool call answered on its own stream (DMN-155).
pub(crate) enum Reply {
    Value(Value),
    Tool(ToolCall),
}

/// A started `tools/call`: the tool's body, answered once it settles.
pub(crate) struct ToolCall {
    pub(super) body: ToolFuture,
    pub(super) era: Era,
}

impl ToolCall {
    /// The `tools/call` result (AGT-050): the tool's text, or its error text with `isError`.
    pub(crate) async fn run(self) -> Value {
        let (text, is_error) = match self.body.await {
            Ok(text) => (text, false),
            Err(text) => (text, true),
        };
        decorate(
            json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }),
            self.era,
            false,
        )
    }
}

/// `initialize` (DMN-157): the result, and the client name the session keeps (AGT-051).
pub(crate) fn initialize(params: &Map<String, Value>) -> (Value, String) {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = LEGACY
        .iter()
        .copied()
        .find(|version| Some(*version) == requested)
        .unwrap_or(LEGACY[0]);
    let client = params
        .get("clientInfo")
        .and_then(|info| info.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(UNNAMED_CLIENT)
        .to_owned();
    let result = json!({
        "protocolVersion": version,
        "capabilities": capabilities(),
        "serverInfo": server_info(),
        "instructions": INSTRUCTIONS,
    });
    (result, client)
}

/// Runs `method` for `principal` in `era`.
pub(crate) fn call(
    services: &Services,
    principal: Principal,
    era: Era,
    method: &str,
    params: Map<String, Value>,
) -> Result<Reply, RpcError> {
    let value = |result: Value, cacheable: bool| Ok(Reply::Value(decorate(result, era, cacheable)));
    match method {
        "server/discover" => value(
            json!({
                "supportedVersions": supported(),
                "capabilities": capabilities(),
                "instructions": INSTRUCTIONS,
            }),
            true,
        ),
        "ping" => value(json!({}), false),
        "tools/list" => value(json!({ "tools": tools() }), true),
        "tools/call" => tool_call(services, principal, era, params).map(Reply::Tool),
        "resources/list" => value(
            json!({ "resources": [{
                "uri": METHODS_URL,
                "name": "omfp-protocol-methods",
                "title": "omfp protocol methods",
                "description": "Every method omfp_call accepts, one line each.",
                "mimeType": "text/markdown",
            }] }),
            true,
        ),
        "resources/templates/list" => value(
            json!({ "resourceTemplates": [{
                "uriTemplate": METHOD_TEMPLATE,
                "name": "omfp-protocol-method",
                "title": "omfp protocol method",
                "description": "One method's description and its params and result JSON Schemas.",
                "mimeType": "application/json",
            }] }),
            true,
        ),
        "resources/read" => value(read(&params)?, true),
        _ => Err(RpcError::new(
            match era {
                Era::Modern => StatusCode::NOT_FOUND,
                Era::Legacy => StatusCode::OK,
            },
            code::METHOD_NOT_FOUND,
            format!("Method not found: `{method}`"),
        )),
    }
}

/// The registry's tools as MCP tools (AGT-050), in registry order.
fn tools() -> Vec<Value> {
    registry()
        .into_iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "title": tool.label,
                "description": tool.description,
                "inputSchema": tool.parameters,
            })
        })
        .collect()
}

/// Starts `tools/call { name, arguments }` as the MCP principal, without a workspace (AGT-050, AGT-051).
fn tool_call(
    services: &Services,
    principal: Principal,
    era: Era,
    mut params: Map<String, Value>,
) -> Result<ToolCall, RpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let Some(tool) = registry().into_iter().find(|tool| tool.name == name) else {
        return Err(RpcError::invalid_params(format!(
            "Unknown tool: `{name}`; tools/list lists the tools"
        )));
    };
    let arguments = match params.remove("arguments") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(arguments)) => arguments,
        Some(_) => {
            return Err(RpcError::invalid_params(format!(
                "the arguments of `{name}` must be an object"
            )));
        }
    };
    let context = ToolContext {
        services: services.clone(),
        principal,
        workspace_id: None,
    };
    Ok(ToolCall {
        body: (tool.run)(context, arguments),
        era,
    })
}

/// `resources/read { uri }` (AGT-052): a catalogue URL, read as the host URI is.
fn read(params: &Map<String, Value>) -> Result<Value, RpcError> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let read = catalog::read_uri(uri)
        .ok_or_else(|| {
            RpcError::invalid_params(format!(
                "Resource not found: `{uri}`; resources/list and resources/templates/list name the resources"
            ))
        })?
        .map_err(RpcError::invalid_params)?;
    let mime_type = match read.content_type {
        Some(HostUriResultContentType::ApplicationJson) => "application/json",
        _ => "text/markdown",
    };
    Ok(json!({ "contents": [{ "uri": uri, "mimeType": mime_type, "text": read.content }] }))
}

fn capabilities() -> Value {
    json!({
        "tools": { "listChanged": false },
        "resources": { "subscribe": false, "listChanged": false },
    })
}

fn server_info() -> Value {
    json!({ "name": "omfp", "title": "omfp", "version": env!("CARGO_PKG_VERSION") })
}

/// A result as `era` wants it: the current revision adds `resultType`, the server's identity and, for a cacheable
/// result, the caching hints (DMN-156).
fn decorate(mut result: Value, era: Era, cacheable: bool) -> Value {
    if era == Era::Modern
        && let Value::Object(fields) = &mut result
    {
        fields.insert("resultType".to_owned(), json!("complete"));
        fields.insert(
            "_meta".to_owned(),
            json!({ "io.modelcontextprotocol/serverInfo": server_info() }),
        );
        if cacheable {
            fields.insert("ttlMs".to_owned(), json!(TTL_MS));
            fields.insert("cacheScope".to_owned(), json!("private"));
        }
    }
    result
}
