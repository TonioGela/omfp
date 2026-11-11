-- The activity log (AGT-072, DMN-160): one row per call an agent principal made beyond reading, with its outcome. No
-- foreign key: entries outlive their thread. Only the oldest rows are ever deleted, so `seq` (the rowid) keeps
-- increasing.

CREATE TABLE activity (
    seq INTEGER PRIMARY KEY,
    at INTEGER NOT NULL,
    thread_id TEXT NOT NULL,
    client TEXT NOT NULL,
    method TEXT NOT NULL,
    params TEXT NOT NULL,
    error TEXT
);

CREATE INDEX activity_by_thread ON activity (thread_id, seq);
