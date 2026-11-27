-- Immutable admission plus locked artifacts. Generic tables enforce scoped reads.
CREATE TABLE forecast_runs (
 contest_id TEXT PRIMARY KEY REFERENCES contests(contest_id) ON DELETE CASCADE,
 setup_json TEXT NOT NULL CHECK(json_valid(setup_json)),
 case_id TEXT NOT NULL,
 frame_json TEXT CHECK(frame_json IS NULL OR json_valid(frame_json)),
 first_aggregate_json TEXT CHECK(first_aggregate_json IS NULL OR json_valid(first_aggregate_json)),
 final_aggregate_json TEXT CHECK(final_aggregate_json IS NULL OR json_valid(final_aggregate_json))
) STRICT;
CREATE TABLE forecast_records (
 contest_id TEXT NOT NULL REFERENCES contests(contest_id) ON DELETE CASCADE,
 record_key TEXT NOT NULL,
 body TEXT NOT NULL CHECK(json_valid(body)),
 event_seq INTEGER REFERENCES events(seq),
 PRIMARY KEY(contest_id,record_key)
) STRICT;
CREATE TABLE forecast_work_inputs (
 work_id TEXT PRIMARY KEY REFERENCES work(work_id) ON DELETE CASCADE,
 input_json TEXT NOT NULL CHECK(json_valid(input_json))
) STRICT;
