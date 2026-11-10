//! Types of the UI <-> daemon protocol (docs/specs/03-protocol.md).
//!
//! Shared by the daemon and every client; compiles to `wasm32-unknown-unknown` (ARC-010).
//!
//! A client sends [`Request`]s (decode them with [`decode_request`]); the daemon answers with [`ServerFrame`]s. Each
//! method has a marker type in [`method`] implementing [`Method`], which names its params and result types.

mod dto;
mod envelope;
pub mod method;
mod params;
mod schema;
mod topic;

pub use dto::{
    CreatedBy, Density, DiffPage, FileChange, FileDiff, FileStatus, FontSettings, GitStatus,
    IndexSnapshot, KeymapEntry, KeymapMode, NotificationMode, PathEntry, Provenance, RepoKind,
    RepoStatus, SeenRepository, Settings, TerminalInfo, TerminalKind, Theme, ThreadSummary,
    TitleSource, UiSettings, Workspace, WorkspaceRepo,
};
pub use envelope::{
    ErrorBody, ErrorCode, EventFrame, IndexEvent, ProtocolError, Request, Response, ServerFrame,
    TopicEvent, WorkspaceEvent, decode_request,
};
pub use method::{Call, METHODS, Method, method_schemas};
pub use params::{
    AddRepoParams, BashParams, BtwParams, CompactParams, CompletePathParams, CreateThreadParams,
    CreateWorkspaceParams, Empty, ForkParams, GoalParams, HelloParams, HelloResult,
    InteractionResponse, LabelParams, ListTerminalsParams, MessageParams, NavigateParams,
    OpenTerminalParams, OpenTerminalResult, PathCompletions, Position, PromptBehavior,
    PromptParams, PromptResult, QueueMessageParams, RemoveRepoParams, RemoveWorkspaceParams,
    RepoRef, RepoSource, RepoSuggestions, ResizeTerminalParams, RespondParams, SearchThreadsParams,
    SetModelParams, SetSettingsParams, SetThinkingParams, SubagentMessagesParams, SubagentRef,
    SubagentSteerParams, SubscribeParams, SubscribeResult, SuggestReposParams, TerminalList,
    TerminalRef, ThreadGit, ThreadRef, ThreadSearchResult, ThreadSearchResults, ThreadWorkspace,
    TopicParams, UpdateThreadParams, UpdateWorkspaceParams, WorkspaceDiffParams, WorkspaceRef,
};
pub use schema::type_schemas;
pub use topic::{Topic, TopicParseError};

/// Protocol version announced in `system.hello` (PRT-004).
pub const PROTOCOL_VERSION: u32 = 1;

/// Largest text frame either side may send (PRT-013); bigger payloads are paged or truncated.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
