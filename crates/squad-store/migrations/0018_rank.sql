CREATE TABLE rank_runs (
  contest_id TEXT PRIMARY KEY REFERENCES contests(contest_id) ON DELETE CASCADE,
  setup_json TEXT NOT NULL CHECK(json_valid(setup_json)),
  run_json TEXT NOT NULL CHECK(json_valid(run_json))
);
