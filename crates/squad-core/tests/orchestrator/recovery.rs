use super::support::*;
use squad_core::orchestrator::{
    machine,
    recovery::{self, OpenMode, RecoveryAction},
    stop,
    types::*,
};
use squad_protocol::*;

#[test]
fn restart_after_dispatch_and_one_result_does_not_repeat_accepted_call() {
    let p = plan(2, false, true);
    let s = state(&p, DebateStatus::RoundRunning);
    let a = attempt(work(&p, 0, 1), AttemptPhase::Accepted(EventId::new()));
    let b = attempt(work(&p, 1, 1), AttemptPhase::Queued);
    let recovered = recovery::recover(
        &recovery::RecoverySnapshot {
            state: &s,
            plan: None,
            attempts: &[a, b.clone()],
            barrier_committed: false,
            final_work_exists: false,
            committed_rounds: &[],
            scores: &[],
            manual_judge: false,
        },
        now(),
        OpenMode::Interactive,
    );
    assert_eq!(
        recovered,
        vec![
            RecoveryAction::RestoreHolds,
            RecoveryAction::Attempt(AttemptDecision::Wake(b.work.work_id))
        ]
    );
}
#[test]
fn restart_before_and_after_barrier_and_stop() {
    let p = plan(2, false, true);
    let mut s = state(&p, DebateStatus::RoundCommitting);
    assert!(
        recovery::recover(
            &recovery::RecoverySnapshot {
                state: &s,
                plan: None,
                attempts: &[],
                barrier_committed: false,
                final_work_exists: false,
                committed_rounds: &[],
                scores: &[],
                manual_judge: false
            },
            now(),
            OpenMode::Interactive
        )
        .contains(&RecoveryAction::CommitBarrier)
    );
    assert!(
        !recovery::recover(
            &recovery::RecoverySnapshot {
                state: &s,
                plan: None,
                attempts: &[],
                barrier_committed: true,
                final_work_exists: false,
                committed_rounds: &[],
                scores: &[],
                manual_judge: false
            },
            now(),
            OpenMode::Interactive
        )
        .contains(&RecoveryAction::CommitBarrier)
    );
    s.state = DebateStatus::Stopping;
    assert_eq!(
        recovery::recover(
            &recovery::RecoverySnapshot {
                state: &s,
                plan: None,
                attempts: &[],
                barrier_committed: false,
                final_work_exists: false,
                committed_rounds: &[],
                scores: &[],
                manual_judge: false
            },
            now(),
            OpenMode::Interactive
        ),
        vec![RecoveryAction::FinishStop]
    );
    s.state = DebateStatus::Stopped;
    assert!(
        recovery::recover(
            &recovery::RecoverySnapshot {
                state: &s,
                plan: None,
                attempts: &[],
                barrier_committed: false,
                final_work_exists: false,
                committed_rounds: &[],
                scores: &[],
                manual_judge: false
            },
            now(),
            OpenMode::Interactive
        )
        .is_empty()
    );
}
#[test]
fn confirmation_expiry_and_one_shot_wake_rules() {
    let p = plan(1, false, false);
    let s = state(&p, DebateStatus::AwaitingConfirmation);
    assert_eq!(
        recovery::recover(
            &recovery::RecoverySnapshot {
                state: &s,
                plan: Some(&p.preflight),
                attempts: &[],
                barrier_committed: false,
                final_work_exists: false,
                committed_rounds: &[],
                scores: &[],
                manual_judge: false
            },
            now(),
            OpenMode::Interactive
        ),
        vec![RecoveryAction::WaitForConfirmation]
    );
    assert_eq!(
        recovery::recover(
            &recovery::RecoverySnapshot {
                state: &s,
                plan: Some(&p.preflight),
                attempts: &[],
                barrier_committed: false,
                final_work_exists: false,
                committed_rounds: &[],
                scores: &[],
                manual_judge: false
            },
            p.preflight.expires_at,
            OpenMode::Interactive
        ),
        vec![RecoveryAction::Expire]
    );
    assert!(
        recovery::recover(
            &recovery::RecoverySnapshot {
                state: &s,
                plan: Some(&p.preflight),
                attempts: &[],
                barrier_committed: false,
                final_work_exists: false,
                committed_rounds: &[],
                scores: &[],
                manual_judge: false
            },
            now(),
            OpenMode::OneShot(ContestId::new())
        )
        .is_empty()
    );
}
#[test]
fn interrupted_scoring_retries_original_work_and_final_work_is_not_duplicated() {
    let p = plan(1, false, true);
    let mut s = state(&p, DebateStatus::Judging);
    case(&mut s);
    let w = judge_work(&p, &s, Some(1));
    let mut a = attempt(w, AttemptPhase::Running);
    a.deadline = Some(now());
    let actions = recovery::recover(
        &recovery::RecoverySnapshot {
            state: &s,
            plan: None,
            attempts: &[a.clone()],
            barrier_committed: true,
            final_work_exists: false,
            committed_rounds: &[],
            scores: &[],
            manual_judge: false,
        },
        now(),
        OpenMode::Interactive,
    );
    assert!(
        !actions
            .iter()
            .any(|x| matches!(x, RecoveryAction::Attempt(AttemptDecision::Retry { .. })))
    );
    assert!(matches!(
        squad_core::orchestrator::rounds::attempt(&a, now(), false),
        AttemptDecision::Retry {
            next_attempt: 2,
            ..
        }
    ));
    assert!(!actions.contains(&RecoveryAction::FinalJudgeReady));
    a.phase = AttemptPhase::Exhausted;
    assert!(
        !recovery::recover(
            &recovery::RecoverySnapshot {
                state: &s,
                plan: None,
                attempts: &[a.clone()],
                barrier_committed: true,
                final_work_exists: false,
                committed_rounds: &[],
                scores: &[],
                manual_judge: false
            },
            now(),
            OpenMode::Interactive
        )
        .contains(&RecoveryAction::FinalJudgeReady)
    );
    assert!(
        !recovery::recover(
            &recovery::RecoverySnapshot {
                state: &s,
                plan: None,
                attempts: &[a],
                barrier_committed: true,
                final_work_exists: true,
                committed_rounds: &[],
                scores: &[],
                manual_judge: false
            },
            now(),
            OpenMode::Interactive
        )
        .contains(&RecoveryAction::FinalJudgeReady)
    );
    let unscored = DebateRoundScore {
        debate_id: s.debate_id,
        case_id: s.case_id.unwrap(),
        round: 1,
        scores: None,
        tally: s.debaters.iter().cloned().map(|n| (n, 0)).collect(),
        reason: Some(Text::new("unscored").unwrap()),
    };
    let settled = recovery::RecoverySnapshot {
        state: &s,
        plan: None,
        attempts: &[],
        barrier_committed: true,
        final_work_exists: false,
        committed_rounds: &[1],
        scores: &[unscored],
        manual_judge: false,
    };
    assert!(
        recovery::recover(&settled, now(), OpenMode::Interactive)
            .contains(&RecoveryAction::FinalJudgeReady)
    );
}
#[test]
fn participant_stop_fences_round_scores_and_late_results() {
    let p = plan(1, false, true);
    let mut s = state(&p, DebateStatus::RoundRunning);
    case(&mut s);
    let mut router = FakeRouter::new(&p);
    router.state = s.clone();
    let works = vec![work(&p, 0, 1), work(&p, 1, 1), judge_work(&p, &s, Some(1))];
    assert_eq!(stop::work_to_fence(&s, &works).len(), 3);
    assert!(!stop::applies(&s, Some(&nick("outsider"))));
    stop::request(&mut router, EventId::new(), &s, Some(&nick("alpha"))).unwrap();
    assert_eq!(router.state.state, DebateStatus::Stopping);
    assert!(
        machine::advance(
            &router.state,
            &DebateTransition::RoundScoreAccepted {
                score_event_id: EventId::new()
            },
            false
        )
        .is_err()
    );
}
#[test]
fn manual_rejection_cancellation_expiry_restore_complete_prior_snapshot() {
    let p = plan(2, false, true);
    let mut prior = state(&p, DebateStatus::Stopped);
    case(&mut prior);
    prior.terminal_reason = Some("stopped".into());
    let preparing = machine::advance(&prior, &DebateTransition::JudgeOnlyRequested, false).unwrap();
    assert_eq!(
        machine::restore_manual(&preparing, &prior, &DebateTransition::JudgeOnlyPlanRejected)
            .unwrap()
            .case_ids,
        prior.case_ids
    );
    let awaiting =
        machine::advance(&preparing, &DebateTransition::JudgeOnlyPlanReady, false).unwrap();
    for t in [
        DebateTransition::JudgeOnlyCancelled,
        DebateTransition::JudgeOnlyExpired,
    ] {
        let restored = machine::restore_manual(&awaiting, &prior, &t).unwrap();
        assert_eq!(restored.state, prior.state);
        assert_eq!(restored.case_id, prior.case_id);
        assert_eq!(restored.terminal_reason, prior.terminal_reason);
        assert!(restored.state_version > awaiting.state_version);
    }
}

#[test]
fn manual_pending_plan_roundtrips_prior_snapshot_without_losing_case_ids() {
    let mut p = plan(2, false, true);
    let mut prior = state(&p, DebateStatus::Stopped);
    case(&mut prior);
    p.preflight.judge_only = true;
    p.prior_terminal = Some(prior.clone());
    squad_core::orchestrator::preflight::freeze(
        &mut p,
        squad_core::orchestrator::preflight::ContextBounds {
            members: [100, 100],
            judge: 100,
        },
        100.0,
        100.0,
        now(),
    )
    .unwrap();
    let encoded = squad_core::orchestrator::provenance::encode_plan(&p).unwrap();
    let restored: squad_core::orchestrator::types::DebatePlan =
        serde_json::from_value(encoded).unwrap();
    let mut awaiting = prior.clone();
    awaiting.state = DebateStatus::AwaitingConfirmation;
    awaiting.state_version += 2;
    for t in [
        DebateTransition::JudgeOnlyPlanRejected,
        DebateTransition::JudgeOnlyCancelled,
        DebateTransition::JudgeOnlyExpired,
    ] {
        let state =
            machine::restore_manual(&awaiting, restored.prior_terminal.as_ref().unwrap(), &t)
                .unwrap();
        assert_eq!(state.case_ids, prior.case_ids);
        assert_eq!(state.state, prior.state);
    }
}
