//! Versioned lifecycle decisions. The router remains the authoritative guard.
use super::types::ContestRouter;
use squad_protocol::*;

pub fn submission(
    state: &DebateState,
    transition: DebateTransition,
) -> ContestTransitionSubmission {
    ContestTransitionSubmission {
        contest_id: state.debate_id,
        kind: ContestKind::Debate,
        expected_state_version: state.state_version,
        transition: ContestTransition::Debate(transition),
    }
}
/// Submit a previously persisted operation ID. Retrying must use this same ID.
pub fn submit<R: ContestRouter>(
    router: &mut R,
    id: EventId,
    state: &DebateState,
    transition: DebateTransition,
) -> squad_store::Result<squad_store::commit::CommitOutcome> {
    router.transition(id, submission(state, transition))
}
/// Preview the lifecycle effect. Artifact existence, holds, reservations, round
/// readiness and case allocation are checked inside the router transaction.
pub fn advance(
    state: &DebateState,
    transition: &DebateTransition,
    opening: bool,
) -> Result<DebateState, ErrorCode> {
    use DebateStatus as S;
    use DebateTransition as T;
    state
        .validate()
        .map_err(|_| ErrorCode::INVALID_STATE_TRANSITION)?;
    let mut next = state.clone();
    next.state = match (&state.state, transition) {
        (S::Preparing, T::Prepare) => S::Preparing,
        (S::Preparing, T::PlanReady | T::JudgeOnlyPlanReady) => S::AwaitingConfirmation,
        (S::Preparing, T::PlanRejected) => S::Rejected,
        (S::AwaitingConfirmation, T::Confirm | T::WaitForDebaters) => S::WaitingForDebaters,
        (S::AwaitingConfirmation, T::Cancel) => S::Cancelled,
        (S::AwaitingConfirmation, T::Expire) => S::Expired,
        (S::WaitingForDebaters, T::DebatersReady) => {
            next.round = u32::from(!opening);
            S::RoundRunning
        }
        (S::RoundRunning, T::RoundEntered | T::SlotResolved { .. }) => S::RoundRunning,
        (S::RoundRunning, T::RoundResolved) => S::RoundCommitting,
        (S::RoundCommitting, T::OpeningBarrierCommitted) if state.round == 0 => {
            next.round = 1;
            S::RoundRunning
        }
        (S::RoundCommitting, T::NumberedBarrierCommitted)
            if state.round > 0 && state.round < state.total_rounds =>
        {
            next.round += 1;
            S::RoundRunning
        }
        (S::RoundCommitting, T::JudgedFinalBarrierCommitted)
            if state.round == state.total_rounds && state.judge_requested =>
        {
            S::Judging
        }
        (S::RoundCommitting, T::UnjudgedFinalBarrierCommitted)
            if state.round == state.total_rounds && !state.judge_requested =>
        {
            S::CompleteUnjudged
        }
        (S::Judging, T::VerdictAccepted { .. }) => S::Completed,
        (S::Judging, T::FinalJudgingFailed) => S::Failed,
        (
            S::RoundRunning | S::RoundCommitting | S::Judging,
            T::RoundScoreAccepted { .. } | T::RoundScoreExhausted { .. },
        ) if state.judge_requested => state.state.clone(),
        (S::Judging, T::RoundScoringFinished) => S::Judging,
        (S::Stopping, T::StopCommitted) => S::Stopped,
        (s, T::StopRequested) if !s.is_terminal() && *s != S::Stopping => S::Stopping,
        (s, T::UnrecoverableFailure) if !s.is_terminal() => S::Failed,
        (S::Stopped | S::CompleteUnjudged | S::Failed, T::JudgeOnlyRequested) => S::Preparing,
        (S::AwaitingConfirmation, T::JudgeOnlyConfirmed) => S::Judging,
        _ => return Err(ErrorCode::INVALID_STATE_TRANSITION),
    };
    next.state_version = next
        .state_version
        .checked_add(1)
        .ok_or(ErrorCode::INTERNAL_ERROR)?;
    next.validate()
        .map_err(|_| ErrorCode::INVALID_STATE_TRANSITION)?;
    Ok(next)
}
/// Rejection, cancellation and expiry of manual judging restore the complete
/// retained terminal snapshot, with a fresh monotonic state version.
pub fn restore_manual(
    state: &DebateState,
    prior: &DebateState,
    transition: &DebateTransition,
) -> Result<DebateState, ErrorCode> {
    use DebateTransition as T;
    let allowed = match transition {
        T::JudgeOnlyPlanRejected => matches!(
            state.state,
            DebateStatus::Preparing | DebateStatus::AwaitingConfirmation
        ),
        T::JudgeOnlyCancelled | T::JudgeOnlyExpired => {
            state.state == DebateStatus::AwaitingConfirmation
        }
        _ => false,
    };
    if !allowed
        || prior.debate_id != state.debate_id
        || !matches!(
            prior.state,
            DebateStatus::Stopped | DebateStatus::CompleteUnjudged | DebateStatus::Failed
        )
    {
        return Err(ErrorCode::INVALID_STATE_TRANSITION);
    }
    let mut restored = prior.clone();
    restored.state_version = state
        .state_version
        .checked_add(1)
        .ok_or(ErrorCode::INTERNAL_ERROR)?;
    Ok(restored)
}
