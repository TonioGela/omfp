-- Workspace folders (D-11): a workspace is a folder holding repositories, and each thread lives in one workspace.
-- The rows (one new owned folder per existing thread) are written by the daemon between the two parts of this
-- migration, see `store::migrate_to_folders`.

-- part: schema
DROP TABLE thread_workspaces;

DROP TABLE workspaces;

CREATE TABLE workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    path TEXT NOT NULL UNIQUE,
    owned INTEGER NOT NULL CHECK (owned IN (0, 1)),
    created_by TEXT NOT NULL,
    created_client TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE threads_v3 (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    workspace_id TEXT NOT NULL REFERENCES workspaces (id),
    session_dir TEXT NOT NULL,
    omp_session_id TEXT,
    approval_mode TEXT,
    model TEXT,
    thinking_level TEXT,
    created_by TEXT NOT NULL,
    created_client TEXT,
    created_at INTEGER NOT NULL,
    archived_at INTEGER
);

-- part: swap
DROP TABLE threads;

ALTER TABLE threads_v3 RENAME TO threads;

CREATE INDEX threads_workspace ON threads (workspace_id);

-- The `--add-dir` roots last given to each thread's omp session (DMN-029).
CREATE TABLE thread_roots (
    thread_id TEXT NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    PRIMARY KEY (thread_id, path)
);
