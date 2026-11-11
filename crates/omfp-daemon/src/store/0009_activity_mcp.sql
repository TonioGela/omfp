-- The MCP principal's calls in the activity log (AGT-053, DMN-160): `thread_id` becomes nullable (`NULL` for a call of
-- an MCP client outside omfp). SQLite cannot drop a NOT NULL constraint, so the table is rebuilt with its rows and
-- their `seq`.

CREATE TABLE activity_new (
    seq INTEGER PRIMARY KEY,
    at INTEGER NOT NULL,
    thread_id TEXT,
    client TEXT NOT NULL,
    method TEXT NOT NULL,
    params TEXT NOT NULL,
    error TEXT
);

INSERT INTO activity_new (seq, at, thread_id, client, method, params, error)
SELECT seq, at, thread_id, client, method, params, error FROM activity;

DROP TABLE activity;

ALTER TABLE activity_new RENAME TO activity;

CREATE INDEX activity_by_thread ON activity (thread_id, seq);
