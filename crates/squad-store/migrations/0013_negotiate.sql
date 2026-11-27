-- Work-bound projections never enter ordinary member history.
CREATE TABLE negotiate_inputs (
 work_id TEXT PRIMARY KEY REFERENCES work(work_id) ON DELETE CASCADE,
 input_json TEXT NOT NULL CHECK(json_valid(input_json))
) STRICT;
CREATE TABLE negotiate_journal (
 contest_id TEXT NOT NULL REFERENCES contests(contest_id) ON DELETE CASCADE,
 key TEXT NOT NULL,
 value_json TEXT NOT NULL CHECK(json_valid(value_json)),
 PRIMARY KEY(contest_id,key)
) STRICT;
