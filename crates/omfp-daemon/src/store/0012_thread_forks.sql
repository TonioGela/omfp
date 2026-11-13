-- Where a forked thread comes from (DMN-300, PRT-181): the source thread (no foreign key: deleting the source keeps
-- the lineage, its title as it was), the omp session entry the fork ends at (`NULL`: the whole session) and the first
-- line of that entry's message. A deleted fork takes its row along.

CREATE TABLE thread_forks (
    thread_id TEXT PRIMARY KEY REFERENCES threads (id) ON DELETE CASCADE,
    source_thread_id TEXT NOT NULL,
    source_title TEXT NOT NULL,
    entry_id TEXT,
    message TEXT
);
