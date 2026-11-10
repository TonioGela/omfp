-- Initial schema (02-daemon, "Persistence"). Timestamps are Unix milliseconds.

CREATE TABLE repositories (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    default_base_ref TEXT NOT NULL,
    remote_url TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('managed_worktree', 'adopted')),
    repository_id TEXT REFERENCES repositories (id) ON DELETE SET NULL,
    branch TEXT,
    base_ref TEXT,
    pr_url TEXT,
    pr_number INTEGER,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    removed_at INTEGER
);

-- A path is unique among live workspaces; a removed one may be created again.
CREATE UNIQUE INDEX workspaces_live_path ON workspaces (path) WHERE removed_at IS NULL;

CREATE TABLE threads (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    session_dir TEXT NOT NULL,
    omp_session_id TEXT,
    approval_mode TEXT,
    model TEXT,
    thinking_level TEXT,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    archived_at INTEGER
);

CREATE TABLE thread_workspaces (
    thread_id TEXT NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    PRIMARY KEY (thread_id, workspace_id)
);

CREATE INDEX thread_workspaces_workspace ON thread_workspaces (workspace_id);

CREATE TABLE thread_tags (
    thread_id TEXT NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    PRIMARY KEY (thread_id, tag)
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
