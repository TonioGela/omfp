//! Types of the UI <-> daemon protocol (docs/specs/03-protocol.md).
//!
//! Shared by the daemon and every client; compiles to `wasm32-unknown-unknown` (ARC-010).
//!
//! A client sends [`Request`]s (decode them with [`decode_request`]); the daemon answers with [`ServerFrame`]s. Each
//! method has a marker type in [`method`] implementing [`Method`], which names its params and result types.

mod activity;
mod dto;
mod envelope;
mod lists;
mod mcp;
pub mod method;
mod params;
mod schema;
mod session;
mod topic;
mod windows;

pub use activity::{
    ActivityEntry, ActivityPage, ListActivityParams, PromptHistory, PromptHistoryEntry,
    PromptHistoryParams, StopThreadParams, ThreadsStopped,
};
pub use dto::{
    CreatedBy, Density, DiffPage, FileCandidate, FileCandidates, FileChange, FileContent, FileDiff,
    FileKind, FileMatch, FileStatus, FileWritten, FontSettings, GitStatus, IndexSnapshot, JobInfo,
    JobLine, JobState, JobStatus, JobStream, KeymapEntry, KeymapMode, Motion, NotificationMode,
    PathEntry, Provenance, RepoKind, RepoStatus, SeenRepository, Settings, TerminalInfo,
    TerminalKind, Theme, ThreadSummary, TitleSource, UiSettings, ViewsState, WidgetInfo,
    WidgetSource, Workspace, WorkspaceBinding, WorkspaceRepo,
};
pub use envelope::{
    ErrorBody, ErrorCode, EventFrame, IndexEvent, JobEvent, ProtocolError, Request, Response,
    ServerFrame, TopicEvent, TransientEvent, ViewsEvent, WorkspaceEvent, decode_request,
};
pub use lists::{ListThreadsParams, ThreadList, WorkspaceList};
pub use mcp::{
    McpReconnectParams, McpScope, McpServer, McpServers, McpSource, McpState, McpTransport,
};
pub use method::{Call, METHODS, Method, method_description, method_schemas};
pub use params::{
    AddRepoParams, ApprovalMode, BashParams, BtwParams, CompactParams, CompletePathParams,
    CopyWidgetParams, CreateThreadParams, CreateWorkspaceParams, Empty, FindFileParams, ForkParams,
    GoalParams, HelloParams, HelloResult, InteractionResponse, JobRef, LabelParams,
    ListTerminalsParams, LiveConnectParams, LiveConnected, LiveMuteParams, LiveMuted, LiveStarted,
    MessageParams, NavigateParams, OpenTerminalParams, OpenTerminalResult, PathCompletions,
    Position, PromptBehavior, PromptParams, PromptResult, QueueMessageParams, ReadFileParams,
    RemoveRepoParams, RemoveWorkspaceParams, RepoRef, RepoSource, RepoSuggestions,
    ReportWidgetsParams, ResizeTerminalParams, RespondParams, SearchThreadsParams, SetModelParams,
    SetSettingsParams, SetThinkingParams, SetViewsParams, SetWidgetStateParams, StartJobParams,
    SubagentMessagesParams, SubagentRef, SubagentSteerParams, SubscribeParams, SubscribeResult,
    SuggestReposParams, TerminalList, TerminalRef, ThreadGit, ThreadRef, ThreadSearchResult,
    ThreadSearchResults, ThreadWorkspace, TopicParams, UpdateThreadParams, UpdateWorkspaceParams,
    WidgetCopied, WidgetJobReport, WidgetReport, WidgetReportState, WorkspaceDiffParams,
    WorkspaceRef, WriteFileParams,
};
pub use params::{FileErrorKind, FileErrorReport, ViewErrors, WindowFileErrors};
pub use params::{
    LibraryComponent, MountParams, MountPlace, MountUpdateParams, Mounted, SaveWidgetParams,
    UnmountParams, Unmounted, WidgetLibrary, WidgetSaved,
};
pub use schema::type_schemas;
pub use session::{ContextUsage, SessionTokens, SessionUsage, ThreadSession};
pub use topic::{Topic, TopicParseError};
pub use windows::{
    AnswerWindowParams, CheckWidgetParams, CommandSource, ListWindowsParams, RanInWindow,
    RunFailure, RunInWindowParams, ViewSource, WidgetChecked, WindowAnswer, WindowBinding,
    WindowCommand, WindowList, WindowRef, WindowReport, WindowRequest, WindowState, WindowSummary,
    WindowView,
};

/// Protocol version announced in `system.hello` (PRT-004).
pub const PROTOCOL_VERSION: u32 = 1;

/// Largest text frame either side may send (PRT-013); bigger payloads are paged or truncated.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
