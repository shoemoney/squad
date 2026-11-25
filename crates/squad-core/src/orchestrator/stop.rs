//! Stop is a versioned transition; the router fences every outstanding attempt.
use super::{machine, types::ContestRouter};
use squad_protocol::*;

pub fn applies(state: &DebateState, target: Option<&MemberNick>) -> bool {
    !state.state.is_terminal() && target.is_none_or(|n| state.debaters.contains(n))
}
pub fn request<R: ContestRouter>(
    router: &mut R,
    request_id: EventId,
    state: &DebateState,
    target: Option<&MemberNick>,
) -> squad_store::Result<squad_store::commit::CommitOutcome> {
    if !applies(state, target) {
        return Err(squad_store::StoreError::Domain(
            ErrorCode::NOTHING_TO_STOP,
            "No applicable active debate",
        ));
    }
    machine::submit(router, request_id, state, DebateTransition::StopRequested)
}
pub fn work_to_fence(state: &DebateState, work: &[Work]) -> Vec<WorkId> {
    work.iter()
        .filter(|w| w.contest_id == Some(state.debate_id))
        .map(|w| w.work_id)
        .collect()
}
pub fn selection_reason(state: &ContestSelectionState) -> Option<&'static str> {
    matches!(
        state.state,
        ContestSelectionStatus::CollectingAnswers | ContestSelectionStatus::AwaitingPick
    )
    .then_some("pick_cancelled")
}
