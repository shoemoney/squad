-- Typed snapshots and frozen per-step projections. Shared artifact visibility remains authoritative.
CREATE TABLE decide_records (
 contest_id TEXT NOT NULL REFERENCES contests(contest_id) ON DELETE CASCADE,
 record_key TEXT NOT NULL,
 body TEXT NOT NULL CHECK(json_valid(body)),
 PRIMARY KEY(contest_id,record_key)
) STRICT;
