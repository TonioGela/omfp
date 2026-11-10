-- No repository registry any more (DMN-021): the daemon suggests the repositories it has seen in workspaces instead.
-- The old known repositories are kept as seen ones; every repository discovered in a workspace is recorded by the scan
-- the daemon runs on startup (DMN-023).

CREATE TABLE seen_repositories (
    path TEXT PRIMARY KEY NOT NULL,
    last_seen_at INTEGER NOT NULL
);

INSERT INTO seen_repositories (path, last_seen_at) SELECT path, created_at FROM repositories;

DROP TABLE repositories;
