//! Judge selection, score arithmetic and final readiness use protocol validators.
use super::types::{AttemptDecision, AttemptState, FrozenModel};
use squad_protocol::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn select_model(
    default: &FrozenModel,
    fallbacks: &[FrozenModel],
    members: &[FrozenModel; 2],
) -> Result<(FrozenModel, Option<String>), ErrorCode> {
    let conflict = members
        .iter()
        .find(|m| m.resolved_slug == default.resolved_slug);
    let Some(conflict) = conflict else {
        return Ok((default.clone(), None));
    };
    let chosen = fallbacks
        .iter()
        .find(|f| {
            !f.resolved_slug.is_empty()
                && !members.iter().any(|m| {
                    m.resolved_slug == f.resolved_slug || (f.nick.is_some() && m.nick == f.nick)
                })
        })
        .ok_or(ErrorCode::MODEL_UNAVAILABLE)?;
    let nick = conflict.nick.as_ref().ok_or(ErrorCode::MODEL_UNAVAILABLE)?;
    Ok((
        chosen.clone(),
        Some(format!(
            "Judge model changed to {} because @{nick} is debating.",
            chosen.resolved_slug
        )),
    ))
}
/// Recompute from accepted records, never trust the tally embedded in a model
/// response or add to an event's running tally (late scores can arrive out of order).
pub fn scoring_evidence(
    state: &DebateState,
    records: &[DebateRoundScore],
    committed_rounds: &[u32],
) -> Result<DebateScoringEvidence, ErrorCode> {
    let mut round_tally: BTreeMap<_, u32> =
        state.debaters.iter().cloned().map(|n| (n, 0)).collect();
    let mut rounds = BTreeMap::new();
    for record in records {
        if !state.case_ids.contains(&record.case_id) {
            return Err(ErrorCode::INVALID_REFERENCE);
        }
        let mut source_state = state.clone();
        source_state.case_id = Some(record.case_id);
        record
            .validate_against(&source_state)
            .map_err(|_| ErrorCode::JUDGE_INVALID_VERDICT)?;
        if !committed_rounds.contains(&record.round) {
            return Err(ErrorCode::INVALID_REFERENCE);
        }
        if let Some(previous) = rounds.insert(record.round, record)
            && previous != record
        {
            return Err(ErrorCode::IDEMPOTENCY_CONFLICT);
        }
    }
    let mut unscored_rounds = BTreeSet::new();
    for round in committed_rounds {
        if *round == 0 || *round > state.total_rounds {
            return Err(ErrorCode::INVALID_REFERENCE);
        }
        if !rounds.get(round).is_some_and(|r| r.scores.is_some()) {
            unscored_rounds.insert(*round);
        }
    }
    for record in rounds.values() {
        if let Some(scores) = &record.scores {
            for (nick, score) in scores {
                *round_tally
                    .get_mut(nick)
                    .ok_or(ErrorCode::JUDGE_INVALID_VERDICT)? += u32::from(score.score);
            }
        }
    }
    Ok(DebateScoringEvidence {
        round_scores: rounds.into_values().cloned().collect(),
        round_tally,
        unscored_rounds: unscored_rounds.into_iter().collect(),
    })
}
pub fn final_ready(
    state: &DebateState,
    committed: &[u32],
    scores: &[DebateRoundScore],
    pending: &[AttemptState],
    final_work_exists: bool,
    manual: bool,
) -> bool {
    state.state == DebateStatus::Judging
        && state.case_id.is_some()
        && scoring_evidence(state, scores, committed).is_ok()
        && !final_work_exists
        && (manual
            || (committed.len() == state.total_rounds as usize
                && (1..=state.total_rounds).all(|r| committed.contains(&r))
                && committed
                    .iter()
                    .all(|r| scores.iter().any(|s| s.round == *r))))
        && pending.iter().all(|p| {
            p.work.contest_id != Some(state.debate_id)
                || !matches!(p.work.kind, WorkKind::JudgeRound)
                || matches!(
                    p.phase,
                    super::types::AttemptPhase::Accepted(_)
                        | super::types::AttemptPhase::Exhausted
                        | super::types::AttemptPhase::Cancelled
                )
        })
}
#[derive(Debug, Clone, PartialEq)]
pub enum JudgeResult<T> {
    Accepted(T),
    Repair(JudgeRepairData),
    Exhausted(ErrorCode),
}
fn validate<T>(
    attempt: &AttemptState,
    raw: &str,
    now: DateTime,
    validator: impl FnOnce(&serde_json::Value) -> Result<T, squad_protocol::validation::ValidationError>,
) -> JudgeResult<T> {
    let parsed = serde_json::from_str(raw).ok();
    let result = parsed.as_ref().map(validator);
    if let Some(Ok(value)) = result {
        return JudgeResult::Accepted(value);
    }
    // Diagnostics deliberately omit private instance data from validation output.
    let mut failed = attempt.clone();
    failed.phase = super::types::AttemptPhase::Failed;
    match super::rounds::attempt(&failed, now, true) {
        AttemptDecision::Retry {
            next_attempt,
            repair: true,
            ..
        } => JudgeResult::Repair(JudgeRepairData {
            attempt: next_attempt,
            rejected_output: raw.into(),
            validation_errors: vec![
                "Output does not satisfy the canonical task schema or evidence constraints.".into(),
            ],
        }),
        _ => JudgeResult::Exhausted(ErrorCode::JUDGE_INVALID_VERDICT),
    }
}
pub fn round_result(
    state: &DebateState,
    attempt: &AttemptState,
    raw: &str,
    now: DateTime,
) -> Result<JudgeResult<RoundJudgeResponse>, ErrorCode> {
    if matches!(
        attempt.phase,
        super::types::AttemptPhase::Accepted(_)
            | super::types::AttemptPhase::Cancelled
            | super::types::AttemptPhase::Exhausted
    ) || attempt
        .work
        .validate_round_bound(state.total_rounds)
        .is_err()
        || state.case_id.is_none()
        || attempt.work.kind != WorkKind::JudgeRound
        || attempt.work.debate_id != Some(state.debate_id)
        || attempt.work.case_id != state.case_id
        || state.state.is_terminal()
        || state.state == DebateStatus::Stopping
    {
        return Err(ErrorCode::WORK_CANCELLED);
    }
    Ok(validate(attempt, raw, now, |v| {
        verdict::validate_round_response(v, &state.debaters)
    }))
}
pub fn final_result(
    state: &DebateState,
    attempt: &AttemptState,
    raw: &str,
    tally: &BTreeMap<MemberNick, u32>,
    now: DateTime,
) -> Result<JudgeResult<Verdict>, ErrorCode> {
    if matches!(
        attempt.phase,
        super::types::AttemptPhase::Accepted(_)
            | super::types::AttemptPhase::Cancelled
            | super::types::AttemptPhase::Exhausted
    ) || attempt
        .work
        .validate_round_bound(state.total_rounds)
        .is_err()
        || state.case_id.is_none()
        || state.state != DebateStatus::Judging
        || attempt.work.kind != WorkKind::Judge
        || attempt.work.debate_id != Some(state.debate_id)
        || attempt.work.case_id != state.case_id
    {
        return Err(ErrorCode::WORK_CANCELLED);
    }
    Ok(validate(attempt, raw, now, |v| {
        verdict::validate_judge_verdict(v, &state.debaters, tally)
    }))
}
pub fn manual_prior(state: &DebateState) -> Result<DebateState, ErrorCode> {
    if !matches!(
        state.state,
        DebateStatus::Stopped | DebateStatus::CompleteUnjudged | DebateStatus::Failed
    ) {
        return Err(ErrorCode::INVALID_STATE_TRANSITION);
    }
    Ok(state.clone())
}
/// Call only after router acceptance. The caller projects this user-role notice
/// to both participants atomically with case sealing and the score update.
pub fn verdict_notice(
    state: &DebateState,
    verdict: &DebateVerdict,
    evidence: &DebateScoringEvidence,
    pack: &crate::prompts::PromptPack,
) -> Result<String, ErrorCode> {
    verdict
        .validate_against(state, &evidence.round_tally)
        .map_err(|_| ErrorCode::JUDGE_INVALID_VERDICT)?;
    let inputs = crate::prompts::Inputs::from([
        ("debate_id".into(), serde_json::json!(state.debate_id)),
        (
            "judge_model_slug".into(),
            serde_json::json!(verdict.completed.model),
        ),
        (
            "verdict_result".into(),
            serde_json::json!(format!("@{} wins", verdict.verdict.verdict.winner)),
        ),
        ("verdict_json".into(), serde_json::json!(verdict.verdict)),
        (
            "round_scores_json".into(),
            serde_json::json!(evidence.round_scores),
        ),
    ]);
    pack.render_template("debate.verdict", &inputs)
        .map_err(|_| ErrorCode::INTERNAL_ERROR)
}

/// A score enters only the human feed and case; the returned history projection
/// has exactly the case destination. Participant delivery happens at verdict.
pub fn score_projection(
    state: &DebateState,
    event: &CommittedEnvelope,
) -> Result<HistoryProjection, ErrorCode> {
    let EnvelopePayload::DebateRoundScore(score) = &event.content else {
        return Err(ErrorCode::INVALID_REFERENCE);
    };
    score
        .validate_against(state)
        .map_err(|_| ErrorCode::JUDGE_INVALID_VERDICT)?;
    if event.base.recipients != vec![Recipient::Me] {
        return Err(ErrorCode::INVALID_REFERENCE);
    }
    Ok(squad_store::history::attributed(
        event,
        super::cases::context(score.case_id),
        "debate.round_score",
        HistoryRole::Assistant,
        None,
        serde_json::to_string(score).map_err(|_| ErrorCode::INTERNAL_ERROR)?,
    ))
}
pub fn verdict_projections(
    state: &DebateState,
    event: &CommittedEnvelope,
    evidence: &DebateScoringEvidence,
    pack: &crate::prompts::PromptPack,
) -> Result<Vec<HistoryProjection>, ErrorCode> {
    let EnvelopePayload::Verdict(verdict) = &event.content else {
        return Err(ErrorCode::INVALID_REFERENCE);
    };
    let text = verdict_notice(state, verdict, evidence, pack)?;
    Ok(state
        .debaters
        .iter()
        .map(|nick| {
            squad_store::history::attributed(
                event,
                ContextId(nick.to_string()),
                "debate.verdict",
                HistoryRole::User,
                Some("squad".into()),
                text.clone(),
            )
        })
        .collect())
}

/// Turn a validated response (or exhausted invocation) into the canonical commit
/// payload. The router recomputes/revalidates the tally under its write lock.
pub fn round_record(
    state: &DebateState,
    round: u32,
    response: Option<RoundJudgeResponse>,
    accepted: &[DebateRoundScore],
) -> Result<DebateRoundScore, ErrorCode> {
    let mut record = DebateRoundScore {
        debate_id: state.debate_id,
        case_id: state.case_id.ok_or(ErrorCode::CASE_NOT_FOUND)?,
        round,
        scores: response.map(|r| r.scores),
        tally: state.debaters.iter().cloned().map(|n| (n, 0)).collect(),
        reason: None,
    };
    if record.scores.is_none() {
        record.reason = Some(Text::new("unscored").expect("literal"));
    }
    record
        .validate_against(state)
        .map_err(|_| ErrorCode::JUDGE_INVALID_VERDICT)?;
    let accepted_rounds: Vec<_> = accepted.iter().map(|r| r.round).collect();
    scoring_evidence(state, accepted, &accepted_rounds)?;
    if let Some(existing) = accepted.iter().find(|r| r.round == round) {
        if existing.scores == record.scores {
            return Ok(existing.clone());
        }
        return Err(ErrorCode::IDEMPOTENCY_CONFLICT);
    }
    let mut records = accepted.to_vec();
    records.push(record.clone());
    let committed: Vec<_> = records.iter().map(|r| r.round).collect();
    record.tally = scoring_evidence(state, &records, &committed)?.round_tally;
    Ok(record)
}
