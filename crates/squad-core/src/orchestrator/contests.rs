//! Shared ownership and script declarations; all claims commit in the router.
use super::types::{FrozenPrompts, StepSpec};
use squad_protocol::*;

pub fn active_error(kind: ContestKind) -> String {
    format!(
        "A {} is already running. Stop it before starting another.",
        kind.as_str()
    )
}
pub fn check_claim(
    active: Option<(ContestId, ContestKind)>,
    requested: ContestId,
) -> Result<(), String> {
    if let Some((id, kind)) = active
        && id != requested
    {
        return Err(active_error(kind));
    }
    Ok(())
}
pub fn debate_script(
    rounds: u32,
    opening: bool,
    judge: bool,
    prompts: &FrozenPrompts,
) -> Result<Vec<StepSpec>, ErrorCode> {
    if !(u32::from(constants::MIN_ROUNDS)..=u32::from(constants::MAX_ROUNDS)).contains(&rounds) {
        return Err(ErrorCode::INVALID_COMMAND);
    }
    let mut steps = Vec::new();
    for round in u32::from(!opening)..=rounds {
        let composition = crate::prompts::inputs::debate_composition(round, rounds)
            .map_err(|_| ErrorCode::INVALID_COMMAND)?;
        steps.push(StepSpec {
            move_name: ContestMove::Fanout,
            round: Some(round),
            actors: Vec::new(),
            composition: Some(composition.into()),
            word_limit: Some(if round > 0 && round == rounds {
                prompts.final_turn_words
            } else {
                prompts.turn_words
            }),
            attempts: constants::DEBATE_TURN_ATTEMPTS,
            deadline_ms: constants::DEBATE_TURN_DEADLINE_MS,
        });
        steps.push(non_model(ContestMove::Barrier, Some(round)));
        if judge && round > 0 {
            steps.push(StepSpec {
                move_name: ContestMove::Judge,
                round: Some(round),
                actors: vec![Target::Judge],
                composition: Some("judge.round".into()),
                word_limit: None,
                attempts: constants::ROUND_JUDGE_ATTEMPTS,
                deadline_ms: constants::ROUND_JUDGE_DEADLINE_MS,
            });
            steps.push(non_model(ContestMove::Tally, Some(round)));
        }
    }
    if judge {
        steps.push(StepSpec {
            move_name: ContestMove::Judge,
            round: None,
            actors: vec![Target::Judge],
            composition: Some("judge.debate".into()),
            word_limit: None,
            attempts: constants::FINAL_JUDGE_ATTEMPTS,
            deadline_ms: constants::FINAL_JUDGE_DEADLINE_MS,
        });
    }
    steps.push(non_model(ContestMove::Report, None));
    Ok(steps)
}
fn non_model(move_name: ContestMove, round: Option<u32>) -> StepSpec {
    StepSpec {
        move_name,
        round,
        actors: Vec::new(),
        composition: None,
        word_limit: None,
        attempts: 0,
        deadline_ms: 0,
    }
}

/// Bind member fanouts to the admitted actors; no step receives room-wide input.
pub fn admitted_script(plan: &super::types::DebatePlan) -> Result<Vec<StepSpec>, ErrorCode> {
    let mut steps = debate_script(
        plan.preflight.rounds,
        plan.preflight.opening_round_required,
        plan.preflight.judge_requested,
        &plan.prompts,
    )?;
    for step in &mut steps {
        if step.move_name == ContestMove::Fanout {
            step.actors = plan
                .preflight
                .debaters
                .iter()
                .cloned()
                .map(Target::Member)
                .collect();
        }
    }
    Ok(steps)
}

/// An adapter reads these observations in one consistent router snapshot. Every
/// action refers to durable work/artifacts; none creates IDs during recovery.
pub struct ExecutionSnapshot<'a> {
    pub state: &'a DebateState,
    pub round_work: &'a [super::types::AttemptState],
    pub scoring_work: &'a [super::types::AttemptState],
    pub committed_rounds: &'a [u32],
    pub scores: &'a [DebateRoundScore],
    pub final_work_exists: bool,
    pub manual_judge: bool,
    pub earlier_unfinished: [bool; 2],
}
#[derive(Debug, Clone, PartialEq)]
pub enum EngineAction {
    Wait,
    Transition(DebateTransition),
    AllocateRound(u32),
    CommitRoundBarrier(u32),
    AllocateRoundScore(u32),
    AllocateFinalJudge,
    Attempt(super::types::AttemptDecision),
}
/// Round progress takes priority over slow scoring. A scored round is allocated
/// only once, using the durable work existence check and accepted score records.
pub fn next(snapshot: &ExecutionSnapshot<'_>, _now: DateTime) -> Result<EngineAction, ErrorCode> {
    use super::types::{AttemptDecision, AttemptPhase};
    let state = snapshot.state;
    state
        .validate()
        .map_err(|_| ErrorCode::INVALID_STATE_TRANSITION)?;
    if state.state.is_terminal() {
        return Ok(EngineAction::Wait);
    }
    if state.state == DebateStatus::Stopping {
        return Ok(EngineAction::Transition(DebateTransition::StopCommitted));
    }
    if state.state == DebateStatus::WaitingForDebaters {
        return Ok(
            super::admission::ready_transition(state, snapshot.earlier_unfinished)?
                .map(EngineAction::Transition)
                .unwrap_or(EngineAction::Wait),
        );
    }
    if state.state == DebateStatus::RoundCommitting {
        return Ok(EngineAction::CommitRoundBarrier(state.round));
    }
    if state.state == DebateStatus::RoundRunning {
        if snapshot.round_work.is_empty() {
            return Ok(EngineAction::AllocateRound(state.round));
        }
        if snapshot.round_work.len() != 2 {
            return Err(ErrorCode::INVALID_STATE_TRANSITION);
        }
        let mut members = std::collections::BTreeSet::new();
        for slot in snapshot.round_work {
            if slot.work.debate_id != Some(state.debate_id)
                || slot.work.round != Some(state.round)
                || slot.work.kind != WorkKind::DebateTurn
            {
                return Err(ErrorCode::INVALID_REFERENCE);
            }
            let Target::Member(nick) = &slot.work.target else {
                return Err(ErrorCode::INVALID_REFERENCE);
            };
            if !state.debaters.contains(nick) || !members.insert(nick) {
                return Err(ErrorCode::INVALID_REFERENCE);
            }
        }
        // Exhaustion alone is insufficient: a durable forfeit event must exist.
        if snapshot
            .round_work
            .iter()
            .all(|a| matches!(a.phase, AttemptPhase::Accepted(_)))
        {
            return Ok(EngineAction::Transition(DebateTransition::RoundResolved));
        }
        for slot in snapshot.round_work {
            let action = super::rounds::engine_attempt(slot);
            if action != AttemptDecision::Wait {
                return Ok(EngineAction::Attempt(action));
            }
        }
    }
    if state.judge_requested && !snapshot.manual_judge {
        for round in snapshot.committed_rounds {
            if *round == 0 {
                continue;
            }
            if !snapshot.scores.iter().any(|s| s.round == *round)
                && !snapshot
                    .scoring_work
                    .iter()
                    .any(|w| w.work.round == Some(*round))
            {
                return Ok(EngineAction::AllocateRoundScore(*round));
            }
        }
    }
    for work in snapshot.scoring_work {
        if snapshot
            .scores
            .iter()
            .any(|s| Some(s.round) == work.work.round)
        {
            continue;
        }
        let action = super::rounds::engine_attempt(work);
        if action != AttemptDecision::Wait {
            return Ok(EngineAction::Attempt(action));
        }
    }
    if super::judge::final_ready(
        state,
        snapshot.committed_rounds,
        snapshot.scores,
        snapshot.scoring_work,
        snapshot.final_work_exists,
        snapshot.manual_judge,
    ) {
        return Ok(EngineAction::AllocateFinalJudge);
    }
    Ok(EngineAction::Wait)
}

/// Forward only router-valid barrier carriers. Routers revalidate projections,
/// origins, destinations and next Work against admitted assignments atomically.
pub fn commit_barrier<R: super::types::ContestRouter>(
    router: &mut R,
    request_id: EventId,
    barrier: BarrierCommit,
) -> squad_store::Result<squad_store::commit::CommitOutcome> {
    let EnvelopePayload::ContestReport(report) = &barrier.barrier_event.content else {
        return Err(squad_store::StoreError::Domain(
            ErrorCode::INVALID_STATE_TRANSITION,
            "Barrier requires a contest report",
        ));
    };
    if report.contest_id != barrier.contest_id || report.step_id != barrier.step_id {
        return Err(squad_store::StoreError::Domain(
            ErrorCode::INVALID_REFERENCE,
            "Barrier identity mismatch",
        ));
    }
    router.barrier(request_id, barrier)
}

/// Run any mode script until its next durable dependency is unresolved. Every
/// callback commits exactly one engine move before observing the next snapshot.
pub fn drive_script<T, E>(mut next: impl FnMut() -> Result<Option<T>, E>) -> Result<Vec<T>, E> {
    let mut outcomes = Vec::new();
    for _ in 0..256 {
        match next()? {
            Some(outcome) => outcomes.push(outcome),
            None => break,
        }
    }
    Ok(outcomes)
}
