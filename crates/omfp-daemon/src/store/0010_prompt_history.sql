-- The prompt history (DMN-216): each text the user sent from omfp once, with its latest submission's thread,
-- workspace and time. No foreign keys: prompts outlive their thread and workspace. Only the oldest rows are deleted.

CREATE TABLE prompt_history (
    text TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    at INTEGER NOT NULL
);

CREATE INDEX prompt_history_by_time ON prompt_history (at);
