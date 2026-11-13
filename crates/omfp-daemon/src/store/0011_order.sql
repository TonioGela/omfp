-- The order of the left column (DMN-280): workspaces among themselves and threads within their workspace, smallest
-- position first (ties by id). Existing workspaces keep the order the column showed them in (by name, ignoring case,
-- then by id), threads theirs (oldest first).

ALTER TABLE workspaces ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
ALTER TABLE threads ADD COLUMN position INTEGER NOT NULL DEFAULT 0;

UPDATE workspaces SET position = (
    SELECT count(*) FROM workspaces AS other
    WHERE lower(other.name) < lower(workspaces.name)
       OR (lower(other.name) = lower(workspaces.name) AND other.id < workspaces.id)
);

UPDATE threads SET position = (
    SELECT count(*) FROM threads AS other
    WHERE other.workspace_id = threads.workspace_id AND other.id < threads.id
);
