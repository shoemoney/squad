//! Round policies over router-allocated canonical Work.
use super::types::{AttemptDecision, AttemptPhase, AttemptState, DebatePlan};
use squad_protocol::*;
use std::collections::BTreeMap;

/// Frozen input mapping consumed directly by PromptPack::debate_inputs.
pub fn prompt_values(
    plan: &DebatePlan,
    work: &Work,
) -> Result<BTreeMap<String, String>, ErrorCode> {
    work.validate_round_bound(plan.preflight.rounds)
        .map_err(|_| ErrorCode::INVALID_STATE_TRANSITION)?;
    if work.kind != WorkKind::DebateTurn || work.debate_id != Some(plan.preflight.debate_id) {
        return Err(ErrorCode::INVALID_REFERENCE);
    }
    let Target::Member(nick) = &work.target else {
        return Err(ErrorCode::INVALID_REFERENCE);
    };
    let index = plan
        .preflight
        .debaters
        .iter()
        .position(|n| n == nick)
        .ok_or(ErrorCode::INVALID_REFERENCE)?;
    let round = work.round.ok_or(ErrorCode::INVALID_REFERENCE)?;
    if round == 0 && !plan.preflight.opening_round_required {
        return Err(ErrorCode::INVALID_STATE_TRANSITION);
    }
    Ok(BTreeMap::from([
        (
            "opponent".into(),
            plan.preflight.debaters[1 - index].to_string(),
        ),
        ("round".into(), round.to_string()),
        ("rounds".into(), plan.preflight.rounds.to_string()),
        (
            "turn_word_limit".into(),
            if round > 0 && round == plan.preflight.rounds {
                plan.prompts.final_turn_words
            } else {
                plan.prompts.turn_words
            }
            .to_string(),
        ),
    ]))
}
/// Validate both allocations before the router commits either or wakes a mailbox.
/// IDs/boundaries are allocated once in that transaction, never on recovery.
pub fn validate_pair(plan: &DebatePlan, work: &[Work; 2]) -> Result<(), ErrorCode> {
    for (index, item) in work.iter().enumerate() {
        prompt_values(plan, item)?;
        if item.target != Target::Member(plan.preflight.debaters[index].clone())
            || item.attempt != 1
        {
            return Err(ErrorCode::INVALID_STATE_TRANSITION);
        }
    }
    if work[0].round != work[1].round
        || work[0].work_id == work[1].work_id
        || work[0].message_id == work[1].message_id
    {
        return Err(ErrorCode::INVALID_STATE_TRANSITION);
    }
    Ok(())
}
pub fn attempt(state: &AttemptState, now: DateTime, invalid_output: bool) -> AttemptDecision {
    let work = &state.work;
    if matches!(
        state.phase,
        AttemptPhase::Accepted(_) | AttemptPhase::Cancelled
    ) {
        return AttemptDecision::Wait;
    }
    if state.phase == AttemptPhase::Exhausted {
        return match work.kind {
            WorkKind::JudgeRound => AttemptDecision::Unscored(work.work_id),
            WorkKind::Judge => AttemptDecision::JudgeFailed(work.work_id),
            _ => AttemptDecision::Forfeit(work.work_id),
        };
    }
    if state.phase == AttemptPhase::Queued {
        return AttemptDecision::Wake(work.work_id);
    }
    if state.phase == AttemptPhase::Running && state.deadline.is_none_or(|d| now < d) {
        return AttemptDecision::Wait;
    }
    failure_policy(
        work.work_id,
        &work.kind,
        work.attempt,
        attempt_limit(&work.kind),
        state.repair_used,
        if invalid_output {
            super::types::FailureClass::InvalidOutput
        } else if state.phase == AttemptPhase::Running {
            super::types::FailureClass::Timeout
        } else {
            super::types::FailureClass::Transport
        },
    )
}
pub fn attempt_limit(kind: &WorkKind) -> u8 {
    match kind {
        WorkKind::JudgeRound => constants::ROUND_JUDGE_ATTEMPTS,
        WorkKind::Judge | WorkKind::ProbeReport => constants::FINAL_JUDGE_ATTEMPTS,
        _ => constants::DEBATE_TURN_ATTEMPTS,
    }
}
/// Called by the router's Failed handler. This pure policy neither requeues work
/// nor submits requests; the router persists repair data and authorizes attempts.
pub fn failure_policy(
    work_id: WorkId,
    kind: &WorkKind,
    attempt: u32,
    budget: u8,
    repair_used: bool,
    failure: super::types::FailureClass,
) -> AttemptDecision {
    let invalid = failure == super::types::FailureClass::InvalidOutput;
    let ceiling = budget.min(attempt_limit(kind));
    if attempt > 0 && attempt < u32::from(ceiling) && !(invalid && repair_used) {
        AttemptDecision::Retry {
            work_id,
            next_attempt: attempt + 1,
            repair: invalid
                && matches!(
                    kind,
                    WorkKind::Judge | WorkKind::JudgeRound | WorkKind::ProbeReport
                ),
        }
    } else {
        match kind {
            WorkKind::JudgeRound => AttemptDecision::Unscored(work_id),
            WorkKind::Judge | WorkKind::ProbeReport => AttemptDecision::JudgeFailed(work_id),
            _ => AttemptDecision::Forfeit(work_id),
        }
    }
}
/// Lifecycle driving consumes terminal router outcomes. The router, rather than
/// an engine tick or recovery scan, authorizes retries on the existing WorkId.
pub fn engine_attempt(state: &AttemptState) -> AttemptDecision {
    match state.phase {
        AttemptPhase::Queued => AttemptDecision::Wake(state.work.work_id),
        AttemptPhase::Exhausted => failure_policy(
            state.work.work_id,
            &state.work.kind,
            state.work.attempt,
            0,
            state.repair_used,
            super::types::FailureClass::Transport,
        ),
        _ => AttemptDecision::Wait,
    }
}

/// Retry changes only the authorized attempt index. In particular, it cannot
/// acquire a newer history boundary or a new message/work identity. This is a
/// template helper for the router's authorization path, not a submission API.
pub fn retry(state: &AttemptState, next_attempt: u32) -> Result<Work, ErrorCode> {
    let decision = attempt(
        state,
        state.deadline.unwrap_or_else(chrono::Utc::now),
        false,
    );
    if !matches!(decision, AttemptDecision::Retry { next_attempt: n, .. } if n == next_attempt) {
        return Err(ErrorCode::STALE_WORK_ATTEMPT);
    }
    let mut work = state.work.clone();
    work.attempt = next_attempt;
    work.validate().map_err(|_| ErrorCode::STALE_WORK_ATTEMPT)?;
    Ok(work)
}
pub fn forfeit(reason: ForfeitReason) -> DebateTurnOutcome {
    let label = match reason {
        ForfeitReason::Timeout => "timeout",
        ForfeitReason::Error => "error",
    };
    DebateTurnOutcome::Forfeit {
        reason_code: reason,
        text: Text::new(format!("[squad notice] Turn forfeited ({label}).")).expect("literal"),
    }
}
/// Accepted opening outcomes, including explicit forfeits, become setup sources.
pub fn opening_setup(plan: &DebatePlan, turns: &[DebateTurn; 2]) -> Result<DebateSetup, ErrorCode> {
    let mut solutions = Vec::new();
    for (index, turn) in turns.iter().enumerate() {
        if turn.debate_id != plan.preflight.debate_id
            || turn.round != 0
            || turn.speaker != plan.preflight.debaters[index]
        {
            return Err(ErrorCode::INVALID_REFERENCE);
        }
        let text = match &turn.outcome {
            DebateTurnOutcome::Completed { content, .. } => content.clone(),
            DebateTurnOutcome::Forfeit { text, .. } => text.to_string(),
        };
        solutions.push(DebateSolution {
            nick: turn.speaker.clone(),
            message_id: turn.generation.message_id,
            text,
        });
    }
    let setup = DebateSetup {
        debate_id: plan.preflight.debate_id,
        topic_message_id: plan.sources.topic_message_id,
        topic_text: plan.sources.topic.clone(),
        solutions: solutions
            .try_into()
            .map_err(|_| ErrorCode::INVALID_REFERENCE)?,
        instruction: plan.sources.instruction.clone(),
        rounds: plan.preflight.rounds,
        judge_model: plan.preflight.judge_model.clone().unwrap_or_default(),
        judge_swapped: plan.preflight.warning.is_some(),
    };
    setup.validate().map_err(|_| ErrorCode::INVALID_REFERENCE)?;
    Ok(setup)
}

/// Bind a router-allocated turn to the frozen registered composition and limits.
/// Both records must be prepared and committed before either worker is woken.
pub fn prepare_turn(plan: &DebatePlan, mut work: Work) -> Result<Work, ErrorCode> {
    prompt_values(plan, &work)?;
    let round = work.round.ok_or(ErrorCode::INVALID_REFERENCE)?;
    let template = match round {
        0 => "debate.round0.md",
        1 => "debate.round1.md",
        _ => "debate.rebuttal.md",
    };
    work.prompt_ref = Some(super::provenance::prompt_ref(&plan.prompts, template)?);
    work.max_output_tokens = Some(constants::DEBATE_TURN_VISIBLE_TOKENS);
    work.validate()
        .map_err(|_| ErrorCode::INVALID_STATE_TRANSITION)?;
    Ok(work)
}
/// The prompt mapper adds closing exactly once in the final-round composition.
pub fn prepare_judge(plan: &DebatePlan, mut work: Work) -> Result<Work, ErrorCode> {
    if work.debate_id != Some(plan.preflight.debate_id)
        || work.case_id.is_none()
        || work.target != Target::Judge
        || !plan.preflight.judge_requested
    {
        return Err(ErrorCode::INVALID_REFERENCE);
    }
    let (template, visible) = match work.kind {
        WorkKind::JudgeRound if !plan.preflight.judge_only => {
            ("judge.round.md", constants::ROUND_JUDGE_VISIBLE_TOKENS)
        }
        WorkKind::Judge => ("judge.user.md", constants::FINAL_JUDGE_VISIBLE_TOKENS),
        _ => return Err(ErrorCode::INVALID_STATE_TRANSITION),
    };
    work.prompt_ref = Some(super::provenance::prompt_ref(&plan.prompts, template)?);
    work.max_output_tokens = Some(visible);
    work.validate_round_bound(plan.preflight.rounds)
        .map_err(|_| ErrorCode::INVALID_STATE_TRANSITION)?;
    Ok(work)
}

pub fn allocate_pair<R: super::types::ContestRouter>(
    router: &mut R,
    request_id: EventId,
    state: &DebateState,
    plan: &DebatePlan,
    work: [Work; 2],
) -> squad_store::Result<squad_store::commit::CommitOutcome> {
    validate_pair(plan, &work)
        .map_err(|code| squad_store::StoreError::Domain(code, "Invalid round pair"))?;
    if state.debate_id != plan.preflight.debate_id
        || state.state != DebateStatus::RoundRunning
        || work[0].round != Some(state.round)
    {
        return Err(squad_store::StoreError::Domain(
            ErrorCode::INVALID_STATE_TRANSITION,
            "Round allocation differs from state",
        ));
    }
    let prepared = work
        .into_iter()
        .map(|w| prepare_turn(plan, w))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|code| squad_store::StoreError::Domain(code, "Cannot bind frozen round work"))?;
    router.allocate_work(request_id, state.debate_id, state.state_version, prepared)
}
pub fn allocate_judge<R: super::types::ContestRouter>(
    router: &mut R,
    request_id: EventId,
    state: &DebateState,
    plan: &DebatePlan,
    work: Work,
) -> squad_store::Result<squad_store::commit::CommitOutcome> {
    if state.case_id != work.case_id || state.debate_id != plan.preflight.debate_id {
        return Err(squad_store::StoreError::Domain(
            ErrorCode::INVALID_REFERENCE,
            "Judge case differs from state",
        ));
    }
    let work = prepare_judge(plan, work)
        .map_err(|code| squad_store::StoreError::Domain(code, "Cannot bind judge work"))?;
    router.allocate_work(request_id, state.debate_id, state.state_version, vec![work])
}

/// Stable-order numbered-round delivery. The router allocates distinct history
/// positions and deduplicates each (context, original event, projection key).
/// Opening evidence is delivered once by debate.setup instead of copying it here.
pub fn barrier_projections(
    state: &DebateState,
    outcomes: &[CommittedEnvelope; 2],
) -> Result<Vec<HistoryProjection>, ErrorCode> {
    if state.state != DebateStatus::RoundCommitting || state.round == 0 {
        return Err(ErrorCode::INVALID_STATE_TRANSITION);
    }
    let mut ordered = Vec::new();
    for speaker in &state.debaters {
        let matching: Vec<_> = outcomes
            .iter()
            .filter(
                |e| matches!(&e.content, EnvelopePayload::DebateTurn(t) if &t.speaker == speaker),
            )
            .collect();
        if matching.len() != 1 {
            return Err(ErrorCode::INVALID_REFERENCE);
        }
        let event = matching[0];
        let EnvelopePayload::DebateTurn(turn) = &event.content else {
            return Err(ErrorCode::INVALID_REFERENCE);
        };
        if turn.debate_id != state.debate_id || turn.round != state.round {
            return Err(ErrorCode::INVALID_REFERENCE);
        }
        ordered.push((event, turn));
    }
    let mut projections = Vec::new();
    for recipient in &state.debaters {
        for (event, turn) in &ordered {
            let (role, name, content) = match &turn.outcome {
                DebateTurnOutcome::Completed { content, .. } if recipient == &turn.speaker => {
                    (HistoryRole::Assistant, None, content.clone())
                }
                DebateTurnOutcome::Completed { content, .. } => (
                    HistoryRole::User,
                    Some(turn.speaker.to_string()),
                    content.clone(),
                ),
                DebateTurnOutcome::Forfeit { text, .. } => {
                    (HistoryRole::User, Some("squad".into()), text.to_string())
                }
            };
            projections.push(squad_store::history::attributed(
                event,
                ContextId(recipient.to_string()),
                "debate.turn",
                role,
                name,
                content,
            ));
        }
    }
    if state.judge_requested {
        let case_id = state.case_id.ok_or(ErrorCode::CASE_NOT_FOUND)?;
        for (event, turn) in ordered {
            // Retain finish reason, usage and explicit forfeits in case evidence.
            let content = serde_json::to_string(turn).map_err(|_| ErrorCode::INTERNAL_ERROR)?;
            projections.push(squad_store::history::attributed(
                event,
                super::cases::context(case_id),
                "debate.turn",
                HistoryRole::User,
                Some("squad".into()),
                content,
            ));
        }
    }
    Ok(projections)
}

pub fn deadline(work: &Work, started_at: DateTime) -> DateTime {
    let ms = match work.kind {
        WorkKind::JudgeRound => constants::ROUND_JUDGE_DEADLINE_MS,
        WorkKind::Judge => constants::FINAL_JUDGE_DEADLINE_MS,
        _ => constants::DEBATE_TURN_DEADLINE_MS,
    };
    started_at + chrono::Duration::milliseconds(ms as i64)
}
