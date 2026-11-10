-- Every workspace is a folder the daemon made under the workspaces root (DMN-020), so `owned` is gone; tags are gone.
-- Workspaces whose folder is elsewhere are then moved into a new folder by the daemon, see `store::migrate`.

DROP TABLE thread_tags;

ALTER TABLE workspaces DROP COLUMN owned;
