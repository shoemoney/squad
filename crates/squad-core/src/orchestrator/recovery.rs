//! Recovery returns decisions from committed observations; it never creates IDs.
use super::{
    rounds,
    types::{AttemptDecision, AttemptState},
};
use squad_protocol::*;

#[derive(Debug, Clone, Copy)]
pub enum OpenMode {
    Interactive,
    OneShot(ContestId),
}
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryAction {
    Expire,
    RestoreHolds,
    WaitForConfirmation,
    Attempt(AttemptDecision),
    CommitBarrier,
    FinalJudgeReady,
    FinishStop,
}
/// `barrier_committed` and `final_work_exists` must be read from durable rows,
/// along with attempts. Never infer them from mailbox delivery or streamed text.
pub struct RecoverySnapshot<'a> {
    pub state: &'a DebateState,
    pub plan: Option<&'a DebatePreflight>,
    pub attempts: &'a [AttemptState],
    pub barrier_committed: bool,
    pub final_work_exists: bool,
    pub committed_rounds: &'a [u32],
    pub scores: &'a [DebateRoundScore],
    pub manual_judge: bool,
}
pub fn recover(
    snapshot: &RecoverySnapshot<'_>,
    now: DateTime,
    mode: OpenMode,
) -> Vec<RecoveryAction> {
    let RecoverySnapshot {
        state,
        plan,
        attempts,
        barrier_committed,
        final_work_exists,
        ..
    } = snapshot;
    if matches!(mode, OpenMode::OneShot(id) if id != state.debate_id) || state.state.is_terminal() {
        return Vec::new();
    }
    if state.state == DebateStatus::AwaitingConfirmation {
        return vec![if plan.is_some_and(|p| now >= p.expires_at) {
            RecoveryAction::Expire
        } else {
            RecoveryAction::WaitForConfirmation
        }];
    }
    if state.state == DebateStatus::Stopping {
        return vec![RecoveryAction::FinishStop];
    }
    let mut result = Vec::new();
    if super::admission::held_for_contest(state) {
        result.push(RecoveryAction::RestoreHolds);
    }
    for attempt in *attempts {
        if attempt.work.contest_id != Some(state.debate_id) {
            continue;
        }
        if attempt.work.kind == WorkKind::JudgeRound
            && snapshot
                .scores
                .iter()
                .any(|s| Some(s.round) == attempt.work.round)
        {
            continue;
        }
        let decision = rounds::engine_attempt(attempt);
        if decision != AttemptDecision::Wait {
            result.push(RecoveryAction::Attempt(decision));
        }
    }
    if state.state == DebateStatus::RoundCommitting && !*barrier_committed {
        result.push(RecoveryAction::CommitBarrier);
    }
    if super::judge::final_ready(
        state,
        snapshot.committed_rounds,
        snapshot.scores,
        attempts,
        *final_work_exists,
        snapshot.manual_judge,
    ) {
        result.push(RecoveryAction::FinalJudgeReady);
    }
    result
}
/// Collection/pick states have no expiry and never rerun already-resolved slots.
pub fn collection_pending(
    state: &ContestSelectionState,
    resolved: &[MemberNick],
) -> Vec<MemberNick> {
    if state.state != ContestSelectionStatus::CollectingAnswers {
        return Vec::new();
    }
    state
        .addressed
        .iter()
        .filter(|n| !resolved.contains(n))
        .cloned()
        .collect()
}
