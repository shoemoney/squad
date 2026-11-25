CREATE TABLE room_meta (
    room_id TEXT PRIMARY KEY CHECK (room_id = 'main'),
    schema_version INTEGER NOT NULL CHECK (schema_version >= 1),
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    next_event_seq INTEGER NOT NULL CHECK (next_event_seq >= 1),
    next_history_pos INTEGER NOT NULL CHECK (next_history_pos >= 1),
    replay_floor INTEGER CHECK (replay_floor >= 1)
) STRICT;

CREATE TABLE events (
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
        'contest.picked'
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

CREATE INDEX events_kind_seq_idx
    ON events(kind, seq);

CREATE INDEX events_correlation_seq_idx
    ON events(correlation_id, seq);

CREATE TABLE history (
    pos INTEGER PRIMARY KEY CHECK (pos >= 1),
    context_id TEXT NOT NULL,
    event_seq INTEGER NOT NULL REFERENCES events(seq),
    origin_event_id TEXT NOT NULL REFERENCES events(id),
    projection_key TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('system', 'user', 'assistant')),
    name TEXT,
    content TEXT NOT NULL,
    meta TEXT NOT NULL CHECK (
        json_valid(meta) AND json_type(meta) = 'object'
    ),
    excluded INTEGER NOT NULL DEFAULT 0 CHECK (excluded IN (0, 1)),
    UNIQUE (context_id, origin_event_id, projection_key)
) STRICT;

CREATE INDEX history_context_pos_idx
    ON history(context_id, pos);

CREATE INDEX history_event_idx
    ON history(event_seq, context_id);

CREATE TABLE contests (
    contest_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN (
        'debate', 'tournament', 'grill',
        'census', 'crosscheck', 'survey', 'probe'
    )),
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    state_version INTEGER NOT NULL CHECK (state_version >= 0),
    state_json TEXT NOT NULL CHECK (
        json_valid(state_json) AND json_type(state_json) = 'object'
    ),
    active INTEGER NOT NULL CHECK (active IN (0, 1)),
    plan_id TEXT UNIQUE,
    plan_json TEXT CHECK (
        plan_json IS NULL OR json_valid(plan_json)
    ),
    pending_request_json TEXT CHECK (
        pending_request_json IS NULL OR json_valid(pending_request_json)
    ),
    expires_at TEXT,
    updated_seq INTEGER REFERENCES events(seq)
) STRICT;

CREATE UNIQUE INDEX contests_one_active_idx
    ON contests(active) WHERE active = 1;

CREATE TABLE contest_events (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    event_seq INTEGER NOT NULL REFERENCES events(seq),
    PRIMARY KEY (contest_id, event_seq)
) STRICT;

CREATE INDEX contest_events_event_idx
    ON contest_events(event_seq);

CREATE TABLE contest_answers (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    nick TEXT NOT NULL,
    work_id TEXT NOT NULL UNIQUE REFERENCES work(work_id),
    answer_event_seq INTEGER UNIQUE REFERENCES events(seq),
    outcome TEXT NOT NULL CHECK (outcome IN (
        'pending', 'answered', 'failed', 'stopped'
    )),
    PRIMARY KEY (contest_id, nick),
    CHECK (
        (outcome = 'answered' AND answer_event_seq IS NOT NULL)
        OR (outcome <> 'answered' AND answer_event_seq IS NULL)
    )
) STRICT;

CREATE TABLE contest_picks (
    contest_id TEXT PRIMARY KEY REFERENCES contests(contest_id),
    requested_event_seq INTEGER NOT NULL UNIQUE REFERENCES events(seq),
    eligible_json TEXT NOT NULL CHECK (
        json_valid(eligible_json)
        AND json_type(eligible_json) = 'array'
    ),
    required_count INTEGER NOT NULL CHECK (required_count IN (1, 2)),
    picked_json TEXT CHECK (
        picked_json IS NULL
        OR (
            json_valid(picked_json)
            AND json_type(picked_json) = 'array'
        )
    ),
    picked_event_seq INTEGER UNIQUE REFERENCES events(seq),
    CHECK (
        (picked_json IS NULL AND picked_event_seq IS NULL)
        OR (picked_json IS NOT NULL AND picked_event_seq IS NOT NULL)
    )
) STRICT;

CREATE TABLE cases (
    case_id TEXT PRIMARY KEY,
    context_id TEXT NOT NULL UNIQUE,
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    contest_id TEXT REFERENCES contests(contest_id),
    judge_model TEXT NOT NULL,
    material_json TEXT NOT NULL CHECK (
        json_valid(material_json) AND json_type(material_json) = 'object'
    ),
    opened_event_seq INTEGER NOT NULL REFERENCES events(seq),
    decision_event_seq INTEGER UNIQUE REFERENCES events(seq),
    state_json TEXT NOT NULL CHECK (
        json_valid(state_json) AND json_type(state_json) = 'object'
    ),
    CHECK (context_id = 'judge:' || case_id)
) STRICT;

CREATE INDEX cases_contest_idx
    ON cases(contest_id);

CREATE TABLE case_events (
    case_id TEXT NOT NULL REFERENCES cases(case_id),
    event_seq INTEGER NOT NULL REFERENCES events(seq),
    PRIMARY KEY (case_id, event_seq)
) STRICT;

CREATE INDEX case_events_event_idx
    ON case_events(event_seq);

CREATE TABLE work (
    work_id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('reply', 'debate.turn', 'judge', 'judge.round')),
    target TEXT NOT NULL,
    context_id TEXT NOT NULL,
    trigger_event_seq INTEGER NOT NULL REFERENCES events(seq),
    trigger_event_id TEXT NOT NULL REFERENCES events(id),
    message_id TEXT NOT NULL UNIQUE,
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
        (kind IN ('judge', 'judge.round') AND target = 'judge'
            AND case_id IS NOT NULL AND context_id = 'judge:' || case_id)
        OR (kind IN ('reply', 'debate.turn') AND target <> 'judge'
            AND case_id IS NULL AND (
                context_id = target
                OR (kind = 'reply' AND contest_id IS NOT NULL AND step_id IS NOT NULL
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

CREATE TABLE judge_repairs (
    work_id TEXT NOT NULL REFERENCES work(work_id),
    attempt INTEGER NOT NULL CHECK (attempt >= 1),
    case_id TEXT NOT NULL REFERENCES cases(case_id),
    rejected_output TEXT NOT NULL,
    validation_errors_json TEXT NOT NULL CHECK (
        json_valid(validation_errors_json)
        AND json_type(validation_errors_json) = 'array'
    ),
    PRIMARY KEY (work_id, attempt)
) STRICT;

CREATE TABLE round_scores (
    debate_id TEXT NOT NULL REFERENCES contests(contest_id),
    case_id TEXT NOT NULL REFERENCES cases(case_id),
    round INTEGER NOT NULL CHECK (round BETWEEN 1 AND 20),
    work_id TEXT NOT NULL UNIQUE REFERENCES work(work_id),
    event_seq INTEGER NOT NULL UNIQUE REFERENCES events(seq),
    scores_json TEXT CHECK (
        scores_json IS NULL
        OR (
            json_valid(scores_json)
            AND json_type(scores_json) = 'object'
        )
    ),
    tally_json TEXT NOT NULL CHECK (
        json_valid(tally_json) AND json_type(tally_json) = 'object'
    ),
    reason TEXT,
    PRIMARY KEY (debate_id, round),
    CHECK (
        (scores_json IS NULL AND reason IS NOT NULL AND reason = 'unscored')
        OR (scores_json IS NOT NULL AND reason IS NULL)
    )
) STRICT;

CREATE INDEX round_scores_case_idx
    ON round_scores(case_id, round);

CREATE TABLE contest_steps (
    step_id TEXT PRIMARY KEY,
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    ordinal INTEGER NOT NULL CHECK (ordinal >= 1),
    script_version TEXT NOT NULL,
    move TEXT NOT NULL CHECK (move IN (
        'collect', 'pick', 'fanout', 'turns',
        'judge', 'tally', 'barrier', 'report'
    )),
    pass INTEGER CHECK (pass BETWEEN 1 AND 5),
    state_json TEXT NOT NULL CHECK (
        json_valid(state_json) AND json_type(state_json) = 'object'
    ),
    inputs_json TEXT NOT NULL CHECK (
        json_valid(inputs_json) AND json_type(inputs_json) = 'array'
    ),
    committed INTEGER NOT NULL DEFAULT 0 CHECK (committed IN (0, 1)),
    updated_seq INTEGER REFERENCES events(seq),
    UNIQUE (contest_id, ordinal)
) STRICT;

CREATE TABLE contest_assignments (
    step_id TEXT NOT NULL REFERENCES contest_steps(step_id),
    target TEXT NOT NULL,
    context_id TEXT NOT NULL,
    payload_refs_json TEXT NOT NULL CHECK (
        json_valid(payload_refs_json)
        AND json_type(payload_refs_json) = 'array'
    ),
    assignment_json TEXT NOT NULL CHECK (
        json_valid(assignment_json) AND json_type(assignment_json) = 'object'
    ),
    work_id TEXT UNIQUE REFERENCES work(work_id),
    outcome TEXT NOT NULL CHECK (outcome IN (
        'pending', 'completed', 'skipped', 'cancelled'
    )),
    error_json TEXT CHECK (
        error_json IS NULL OR json_valid(error_json)
    ),
    PRIMARY KEY (step_id, target)
) STRICT;

CREATE TABLE mode_artifacts (
    event_seq INTEGER PRIMARY KEY REFERENCES events(seq),
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    step_id TEXT NOT NULL REFERENCES contest_steps(step_id),
    artifact_kind TEXT NOT NULL,
    work_id TEXT UNIQUE REFERENCES work(work_id),
    source_refs_json TEXT NOT NULL CHECK (
        json_valid(source_refs_json)
        AND json_type(source_refs_json) = 'array'
    )
) STRICT;

CREATE INDEX mode_artifacts_contest_step_idx
    ON mode_artifacts(contest_id, step_id, event_seq);

CREATE TABLE contest_private_maps (
    contest_id TEXT NOT NULL REFERENCES contests(contest_id),
    scope_key TEXT NOT NULL,
    map_json TEXT NOT NULL CHECK (
        json_valid(map_json) AND json_type(map_json) = 'object'
    ),
    PRIMARY KEY (contest_id, scope_key)
) STRICT;

CREATE TABLE receipts (
    request_id TEXT PRIMARY KEY,
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    producer_identity TEXT NOT NULL,
    normalized_digest TEXT NOT NULL,
    result_json TEXT NOT NULL CHECK (json_valid(result_json)),
    committed_cursor INTEGER REFERENCES events(seq)
) STRICT;

CREATE INDEX receipts_cursor_idx
    ON receipts(committed_cursor);

CREATE TABLE vars (
    name TEXT PRIMARY KEY CHECK (name IN (
        'ROUNDS', 'GRILL_ROUNDS', 'CENSUS_PASSES'
    )),
    value INTEGER NOT NULL,
    updated_seq INTEGER REFERENCES events(seq),
    CHECK (
        (name = 'ROUNDS' AND value BETWEEN 1 AND 20)
        OR (name = 'GRILL_ROUNDS' AND value BETWEEN 1 AND 10)
        OR (name = 'CENSUS_PASSES' AND value BETWEEN 1 AND 5)
    )
) STRICT;

CREATE TABLE spend_daily (
    day TEXT PRIMARY KEY,
    spent_micros INTEGER NOT NULL CHECK (spent_micros >= 0),
    reserved_micros INTEGER NOT NULL CHECK (reserved_micros >= 0),
    unknown_calls INTEGER NOT NULL CHECK (unknown_calls >= 0)
) STRICT;

CREATE TABLE attempts (
    work_id TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK (attempt >= 1),
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    target TEXT NOT NULL,
    context_id TEXT NOT NULL,
    provider_request_id TEXT,
    model TEXT NOT NULL,
    response_model TEXT,
    day TEXT NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    outcome TEXT NOT NULL CHECK (outcome IN (
        'completed', 'failed', 'cancelled', 'unknown'
    )),
    usage_json TEXT CHECK (
        usage_json IS NULL
        OR (
            json_valid(usage_json)
            AND json_type(usage_json) = 'object'
        )
    ),
    reported_cost_micros INTEGER CHECK (reported_cost_micros >= 0),
    estimated_cost_usd REAL CHECK (estimated_cost_usd >= 0),
    unresolved INTEGER NOT NULL CHECK (unresolved IN (0, 1)),
    debate_id TEXT,
    case_id TEXT,
    contest_id TEXT,
    PRIMARY KEY (work_id, attempt)
) STRICT;

CREATE INDEX attempts_unresolved_idx
    ON attempts(unresolved, epoch);

CREATE INDEX attempts_day_idx
    ON attempts(day);

CREATE TABLE reservations (
    reservation_id TEXT PRIMARY KEY,
    epoch INTEGER NOT NULL CHECK (epoch >= 1),
    work_id TEXT,
    attempt INTEGER CHECK (attempt >= 1),
    debate_id TEXT,
    plan_id TEXT,
    amount_usd REAL NOT NULL CHECK (amount_usd >= 0),
    unresolved INTEGER NOT NULL DEFAULT 0
        CHECK (unresolved IN (0, 1)),
    meta TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(meta)),
    contest_id TEXT,
    day TEXT NOT NULL,
    amount_micros INTEGER NOT NULL CHECK (amount_micros >= 0)
) STRICT;

CREATE INDEX reservations_work_attempt_idx
    ON reservations(work_id, attempt);

CREATE INDEX reservations_plan_idx
    ON reservations(plan_id);

CREATE INDEX reservations_unresolved_idx
    ON reservations(unresolved, epoch);

CREATE INDEX reservations_day_idx
    ON reservations(day);

CREATE TABLE scores (
    nick TEXT PRIMARY KEY,
    wins INTEGER NOT NULL CHECK (wins >= 0),
    losses INTEGER NOT NULL CHECK (losses >= 0),
    trophies INTEGER NOT NULL DEFAULT 0 CHECK (trophies >= 0),
    updated_seq INTEGER REFERENCES events(seq)
) STRICT;

CREATE TABLE catalog (
    slug TEXT PRIMARY KEY,
    context_length INTEGER NOT NULL CHECK (context_length >= 1),
    max_output INTEGER CHECK (max_output >= 1),
    pricing_json TEXT NOT NULL CHECK (json_valid(pricing_json)),
    supported_params_json TEXT NOT NULL CHECK (
        json_valid(supported_params_json)
    ),
    fetched_at TEXT NOT NULL
) STRICT;

CREATE TABLE models (
    nick TEXT PRIMARY KEY,
    role TEXT NOT NULL CHECK (role IN ('member', 'judge')),
    roster_order INTEGER UNIQUE CHECK (
        roster_order IS NULL OR roster_order >= 0
    ),
    requested_slug TEXT NOT NULL,
    resolved_slug TEXT NOT NULL,
    context_length INTEGER NOT NULL CHECK (context_length >= 1),
    pricing_json TEXT NOT NULL CHECK (json_valid(pricing_json)),
    resolved_at TEXT NOT NULL,
    CHECK (
        (role = 'member' AND roster_order IS NOT NULL)
        OR (role = 'judge' AND roster_order IS NULL)
    )
) STRICT;

CREATE TABLE superseded (
    superseded_event_id TEXT PRIMARY KEY REFERENCES events(id),
    superseded_message_id TEXT NOT NULL UNIQUE,
    member TEXT NOT NULL,
    in_reply_to_message_id TEXT NOT NULL,
    replacement_work_id TEXT NOT NULL UNIQUE REFERENCES work(work_id),
    replacement_message_id TEXT NOT NULL UNIQUE,
    marker_event_seq INTEGER NOT NULL UNIQUE REFERENCES events(seq)
) STRICT;

CREATE INDEX superseded_member_idx
    ON superseded(member, marker_event_seq);
