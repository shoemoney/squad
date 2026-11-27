-- Private typed snapshots are separate from audience-filtered contest_artifacts.
CREATE TABLE blow_it_up_runs (
    contest_id TEXT PRIMARY KEY REFERENCES contests(contest_id) ON DELETE CASCADE,
    setup_json TEXT NOT NULL CHECK(json_valid(setup_json)),
    pool_json TEXT CHECK(pool_json IS NULL OR json_valid(pool_json)),
    tally_json TEXT CHECK(tally_json IS NULL OR json_valid(tally_json))
) STRICT;
CREATE TABLE blow_it_up_ballots (
    contest_id TEXT NOT NULL REFERENCES blow_it_up_runs(contest_id) ON DELETE CASCADE,
    nick TEXT NOT NULL,
    stage TEXT NOT NULL CHECK(stage IN ('attack','challenge','rule')),
    body TEXT NOT NULL CHECK(json_valid(body)),
    event_seq INTEGER NOT NULL REFERENCES events(seq),
    PRIMARY KEY(contest_id,nick,stage)
) STRICT;
