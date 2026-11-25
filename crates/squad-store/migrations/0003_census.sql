-- Extend released carrier CHECK constraints without editing migration 0001.
CREATE TABLE events_census (
    seq INTEGER PRIMARY KEY CHECK (seq >= 1),
    id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL CHECK (room_id = 'main'),
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    version INTEGER NOT NULL CHECK (version = 1),
    kind TEXT NOT NULL CHECK (kind IN (
        'user.message',
        'message.final',
        'system.notice',
        'error',
        'var.change',
        'work.state',
        'usage.record',
        'budget.state',
        'debate.preflight',
        'debate.preflight.result',
        'debate.opening',
        'debate.setup',
        'debate.state',
        'debate.turn',
        'verdict',
        'case.opened',
        'case.followup',
        'case.ruling',
        'message.superseded',
        'scores.changed',
        'room.cleared',
        'debate.round_score',
        'contest.setup',
        'contest.state',
        'contest.preflight',
        'contest.preflight.result',
        'contest.step',
        'contest.artifact',
        'contest.report',
        'contest.collecting',
        'contest.pick_requested',
        'contest.picked',
        'tournament.setup','tournament.state','tournament.preflight','tournament.entry','tournament.ranking','tournament.result','tournament.champion',
        'census.setup', 'census.state', 'census.plan', 'census.hunt', 'census.ledger', 'census.rating', 'census.report', 'census.case_material'
    )),
    correlation_id TEXT NOT NULL,
    causation_id TEXT,
    sender TEXT NOT NULL,
    recipients TEXT NOT NULL CHECK (
        json_valid(recipients)
        AND json_type(recipients) = 'array'
        AND json_array_length(recipients) >= 1
    ),
    payload TEXT NOT NULL CHECK (
        json_valid(payload) AND json_type(payload) = 'object'
    ),
    created_at TEXT NOT NULL,
    accepted_at TEXT NOT NULL,
    commit_start_seq INTEGER NOT NULL CHECK (commit_start_seq >= 1),
    commit_index INTEGER NOT NULL CHECK (commit_index >= 0),
    commit_size INTEGER NOT NULL CHECK (commit_size >= 1),
    CHECK (commit_index < commit_size),
    CHECK (seq = commit_start_seq + commit_index),
    UNIQUE (commit_start_seq, commit_index)
) STRICT;
INSERT INTO events_census SELECT * FROM events;
DROP TABLE events;
ALTER TABLE events_census RENAME TO events;
CREATE INDEX events_kind_seq_idx
    ON events(kind, seq);
CREATE INDEX events_correlation_seq_idx
    ON events(correlation_id, seq);
CREATE TABLE work_census (
    work_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('reply', 'debate.turn', 'judge', 'judge.round', 'judge.tournament', 'census.hunt', 'census.rate', 'census.merge', 'census.final')),
    target TEXT NOT NULL,
    context_id TEXT NOT NULL,
    trigger_event_seq INTEGER NOT NULL REFERENCES events(seq),
    trigger_event_id TEXT NOT NULL REFERENCES events(id),
    message_id TEXT NOT NULL UNIQUE,
    in_reply_to_message_id TEXT,
    history_through INTEGER CHECK (history_through >= 1),
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    status TEXT NOT NULL CHECK (status IN (
        'queued', 'running', 'completed', 'failed', 'cancelled'
    )),
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    lease_owner TEXT,
    lease_fence INTEGER NOT NULL DEFAULT 0 CHECK (lease_fence >= 0),
    lease_until TEXT,
    execution_deadline TEXT,
    provider_may_have_started INTEGER NOT NULL DEFAULT 0
        CHECK (provider_may_have_started IN (0, 1)),
    runnable INTEGER NOT NULL DEFAULT 0 CHECK (runnable IN (0, 1)),
    held_by_contest_id TEXT REFERENCES contests(contest_id),
    scheduling_meta TEXT NOT NULL DEFAULT '{}'
        CHECK (json_valid(scheduling_meta)),
    contest_id TEXT REFERENCES contests(contest_id),
    debate_id TEXT REFERENCES contests(contest_id),
    round INTEGER CHECK (round >= 0),
    replay INTEGER CHECK (replay IN (0,1)),
    case_id TEXT REFERENCES cases(case_id),
    prompt_ref TEXT CHECK (
        prompt_ref IS NULL OR json_valid(prompt_ref)
    ),
    max_output_tokens INTEGER CHECK (max_output_tokens >= 1),
    error_json TEXT CHECK (
        error_json IS NULL OR json_valid(error_json)
    ),
    completed_event_seq INTEGER REFERENCES events(seq),
    step_id TEXT REFERENCES contest_steps(step_id),
    CHECK (kind <> 'judge.tournament' OR (replay IS NOT NULL AND round IS NOT NULL AND round >= 1 AND contest_id IS NOT NULL AND debate_id IS NULL AND attempt <= 3 AND max_output_tokens IS NOT NULL AND max_output_tokens=4096)),
    CHECK (kind <> 'reply' OR round IS NULL OR replay IS NOT NULL),
    CHECK (kind <> 'reply' OR in_reply_to_message_id IS NOT NULL),
    CHECK (
        kind <> 'debate.turn'
        OR (debate_id IS NOT NULL AND round IS NOT NULL)
    ),
    CHECK (
        debate_id IS NULL
        OR (contest_id IS NOT NULL AND contest_id = debate_id)
    ),
    CHECK (
        round IS NULL OR contest_id IS NOT NULL
    ),
    CHECK (
        (kind IN ('judge', 'judge.round', 'judge.tournament', 'census.merge', 'census.final') AND target = 'judge'
            AND case_id IS NOT NULL AND context_id = 'judge:' || case_id)
        OR (kind IN ('reply', 'debate.turn', 'census.hunt', 'census.rate') AND target <> 'judge'
            AND case_id IS NULL AND (
                context_id = target
                OR (kind IN ('reply', 'census.hunt', 'census.rate') AND contest_id IS NOT NULL AND step_id IS NOT NULL
                    AND context_id =
                        'contest:' || contest_id || ':' || step_id || ':' || target)
            ))
    ),
    CHECK (
        status <> 'running'
        OR (
            attempt >= 1
            AND history_through IS NOT NULL
            AND lease_owner IS NOT NULL
            AND lease_until IS NOT NULL
        )
    ),
    CHECK (
        provider_may_have_started = 0
        OR attempt >= 1
    ),
    CHECK (
        kind <> 'judge.round'
        OR (
            debate_id IS NOT NULL
            AND round IS NOT NULL
            AND round BETWEEN 1 AND 20
            AND attempt <= 2
            AND max_output_tokens IS NOT NULL
            AND max_output_tokens = 512
        )
    )
) STRICT;
INSERT INTO work_census SELECT * FROM work;
DROP TABLE work;
ALTER TABLE work_census RENAME TO work;
CREATE INDEX work_context_queue_idx
    ON work(context_id, status, runnable, trigger_event_seq, work_id);
CREATE INDEX work_unfinished_lease_idx
    ON work(status, lease_until);
CREATE INDEX work_debate_idx
    ON work(debate_id, round, target);
CREATE INDEX work_contest_idx
    ON work(contest_id, round, target);
CREATE INDEX work_held_contest_idx
    ON work(held_by_contest_id) WHERE held_by_contest_id IS NOT NULL;
CREATE INDEX work_case_idx
    ON work(case_id);
CREATE UNIQUE INDEX work_round_judge_idx
    ON work(debate_id, round) WHERE kind = 'judge.round';
CREATE INDEX work_step_idx
    ON work(step_id, target);

CREATE TABLE census_passes (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    pass INTEGER NOT NULL CHECK (pass BETWEEN 1 AND 5),
    plan_event_seq INTEGER NOT NULL REFERENCES events(seq),
    merged_ledger_event_seq INTEGER REFERENCES events(seq),
    final_ledger_event_seq INTEGER REFERENCES events(seq),
    report_event_seq INTEGER UNIQUE REFERENCES events(seq),
    tally_json TEXT CHECK (
        tally_json IS NULL
        OR (
            json_valid(tally_json)
            AND json_type(tally_json) = 'object'
        )
    ),
    PRIMARY KEY (contest_id, pass)
) STRICT;

CREATE TABLE census_finders (
    contest_id TEXT NOT NULL,
    pass INTEGER NOT NULL,
    gap_id TEXT NOT NULL,
    nick TEXT NOT NULL,
    hunt_event_seq INTEGER NOT NULL REFERENCES events(seq),
    PRIMARY KEY (contest_id, pass, gap_id, nick),
    FOREIGN KEY (contest_id, pass)
        REFERENCES census_passes(contest_id, pass)
) STRICT;

CREATE TABLE census_runs (
 contest_id TEXT PRIMARY KEY REFERENCES contests(contest_id),
 setup_json TEXT NOT NULL CHECK(json_valid(setup_json)),
 current_plan_json TEXT NOT NULL CHECK(json_valid(current_plan_json))
) STRICT;
CREATE TABLE census_work_inputs (
 work_id TEXT PRIMARY KEY REFERENCES work(work_id),
 values_json TEXT NOT NULL CHECK(json_valid(values_json))
) STRICT;

CREATE UNIQUE INDEX tournament_entry_slot
    ON events(json_extract(payload,'$.contestId'),json_extract(payload,'$.round'),json_extract(payload,'$.replay'),json_extract(payload,'$.entrant')) WHERE kind='tournament.entry';
CREATE UNIQUE INDEX tournament_work_slot
    ON work(contest_id,round,replay,target) WHERE kind='judge.tournament' OR (kind='reply' AND round IS NOT NULL AND debate_id IS NULL);
