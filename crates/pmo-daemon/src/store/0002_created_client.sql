-- The `client` of the `system.hello` that created a thread or workspace (PRT-033), next to `created_by` (ARC-009).

ALTER TABLE workspaces ADD COLUMN created_client TEXT;

ALTER TABLE threads ADD COLUMN created_client TEXT;
