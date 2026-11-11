//! `threads.list` and `workspaces.list` (PRT-090, PRT-091): the `index` topic's entities for clients that do not
//! subscribe, such as an agent's `omfp_call` (AGT-041).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dto::{ThreadSummary, Workspace};

/// `threads.list` params (PRT-090).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListThreadsParams {
    /// Only the threads of this workspace (an unknown one lists none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// Archived threads too (default `false`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_archived: bool,
}

/// `threads.list` result (PRT-090).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThreadList {
    /// In the order of the `index` topic.
    pub threads: Vec<ThreadSummary>,
}

/// `workspaces.list` result (PRT-091).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceList {
    /// In the order of the `index` topic.
    pub workspaces: Vec<Workspace>,
}
