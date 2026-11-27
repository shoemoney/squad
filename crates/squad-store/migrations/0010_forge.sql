-- Immutable invocation material is owned by a work row and cleared with it.
CREATE TABLE forge_inputs (
 work_id TEXT PRIMARY KEY REFERENCES work(work_id) ON DELETE CASCADE,
 input_json TEXT NOT NULL CHECK(json_valid(input_json))
) STRICT;
