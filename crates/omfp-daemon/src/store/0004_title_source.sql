-- Where a thread's title comes from (DMN-073): `placeholder` (first words of the first message), `generated` (by the
-- daemon from the conversation) or `set` (by the user or an agent, never replaced). Titles before this migration were
-- given at creation, so they count as set.

ALTER TABLE threads ADD COLUMN title_source TEXT NOT NULL DEFAULT 'set'
    CHECK (title_source IN ('placeholder', 'generated', 'set'));
