-- What widgets keep with `useStored` (DMN-086): one JSON value per workspace, widget instance and key, deleted with its
-- workspace.

CREATE TABLE widget_state (
    workspace_id TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    instance TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (workspace_id, instance, key)
);
