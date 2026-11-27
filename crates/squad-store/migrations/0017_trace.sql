-- Durable mode data is scoped by a cascading contest FK, including private provenance.
CREATE TABLE trace_runs (
 contest_id TEXT PRIMARY KEY REFERENCES contests(contest_id) ON DELETE CASCADE,
 setup_json TEXT NOT NULL CHECK(json_valid(setup_json)),
 data_json TEXT NOT NULL CHECK(json_valid(data_json))
) STRICT;
