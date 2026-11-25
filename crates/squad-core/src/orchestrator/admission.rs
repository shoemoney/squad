//! Hold/queue decisions; live reservation checks are router transactions.
use super::{machine, types::ContestRouter};
use squad_protocol::*;

pub fn ready_transition(
    state: &DebateState,
    earlier_unfinished: [bool; 2],
) -> Result<Option<DebateTransition>, ErrorCode> {
    match state.state {
        DebateStatus::WaitingForDebaters if earlier_unfinished.iter().all(|v| !v) => {
            Ok(Some(DebateTransition::DebatersReady))
        }
        DebateStatus::WaitingForDebaters => Ok(None),
        _ => Err(ErrorCode::INVALID_STATE_TRANSITION),
    }
}
/// Router Confirm rechecks sources, model/prompt snapshots, expiry, live remaining
/// daily amount and atomically claims/reserves/holds. No local budget decrement.
pub fn confirm<R: ContestRouter>(
    router: &mut R,
    request_id: EventId,
    state: &DebateState,
) -> squad_store::Result<squad_store::commit::CommitOutcome> {
    machine::submit(router, request_id, state, DebateTransition::Confirm)
}
/// Check before each *new* provider attempt; an already started attempt remains
/// account-able and completable at the cap. Reservation ownership is router-owned.
pub fn dispatch_allowed(
    spent_usd: f64,
    daily_max_usd: f64,
    already_started: bool,
) -> Result<(), ErrorCode> {
    if ![spent_usd, daily_max_usd]
        .iter()
        .all(|v| v.is_finite() && *v >= 0.0)
    {
        return Err(ErrorCode::CONFIG_INVALID);
    }
    if !already_started && spent_usd >= daily_max_usd {
        return Err(ErrorCode::DAILY_MAX_REACHED);
    }
    Ok(())
}
pub fn held_for_contest(state: &DebateState) -> bool {
    matches!(
        state.state,
        DebateStatus::WaitingForDebaters
            | DebateStatus::RoundRunning
            | DebateStatus::RoundCommitting
            | DebateStatus::Judging
            | DebateStatus::Stopping
    )
}
