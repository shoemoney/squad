use super::support::*;
use squad_core::orchestrator::{admission, contests, machine, preflight, types::ContestRouter};
use squad_protocol::*;

fn freeze(
    p: &mut squad_core::orchestrator::types::DebatePlan,
    threshold: f64,
) -> Option<Confirmation> {
    preflight::freeze(
        p,
        preflight::ContextBounds {
            members: [100, 200],
            judge: 300,
        },
        100.0,
        threshold,
        now(),
    )
    .unwrap()
}
#[test]
fn counts_cover_openings_and_accumulating_scores() {
    for opening in [false, true] {
        for judge in [false, true] {
            let mut p = plan(3, opening, judge);
            freeze(&mut p, 100.0);
            assert_eq!(p.calls.len() as u32, p.preflight.planned_calls());
            assert_eq!(
                p.calls.iter().map(|c| u32::from(c.attempts)).sum::<u32>(),
                p.preflight.reserved_calls()
            );
            let scores: Vec<_> = p
                .calls
                .iter()
                .filter(|c| c.kind == WorkKind::JudgeRound)
                .collect();
            assert_eq!(scores.len(), if judge { 3 } else { 0 });
            for pair in scores.windows(2) {
                assert!(pair[1].input_tokens > pair[0].input_tokens);
            }
            for score in scores {
                assert_eq!(
                    (
                        score.visible_tokens,
                        score.reasoning_tokens,
                        score.attempts,
                        score.deadline_ms
                    ),
                    (512, 1024, 2, 120000)
                );
            }
        }
    }
}
#[test]
fn threshold_compares_estimate_not_reservation() {
    let mut p = plan(2, false, true);
    freeze(&mut p, 100.0);
    let threshold = (p.preflight.estimate.estimated_usd + p.preflight.estimate.reserved_usd) / 2.0;
    assert_eq!(freeze(&mut p, threshold), Some(Confirmation::Automatic));
    assert!(!p.preflight.confirmation_required);
    let exact = p.preflight.estimate.estimated_usd;
    assert_eq!(freeze(&mut p, exact), Some(Confirmation::Automatic));
    assert_eq!(freeze(&mut p, 0.0), None);
}
#[test]
fn confirmation_expiry_changes_and_cli_yes() {
    let mut p = plan(1, true, true);
    freeze(&mut p, 0.0);
    assert_eq!(
        preflight::confirmation(&p, None, false, now(), true),
        Ok(None)
    );
    assert_eq!(
        preflight::confirmation(&p, None, true, now(), true),
        Ok(Some(Confirmation::Preconfirmed))
    );
    let request = Request::DebateConfirm {
        request_id: EventId::new(),
        debate_id: p.preflight.debate_id,
        plan_id: p.preflight.plan_id,
    };
    assert_eq!(
        preflight::confirmation(&p, Some(&request), false, now(), true),
        Ok(Some(Confirmation::Explicit))
    );
    assert_eq!(
        preflight::confirmation(&p, Some(&request), false, p.preflight.expires_at, true),
        Err(ErrorCode::DEBATE_PLAN_EXPIRED)
    );
    assert_eq!(
        preflight::confirmation(&p, Some(&request), false, now(), false),
        Err(ErrorCode::DEBATE_PLAN_STALE)
    );
    let cancel = Request::DebateCancel {
        request_id: EventId::new(),
        debate_id: p.preflight.debate_id,
        plan_id: p.preflight.plan_id,
    };
    assert_eq!(
        preflight::cancellation(&p, &cancel),
        Ok(DebateTransition::Cancel)
    );
}
#[test]
fn preflight_is_emitted_before_automatic_admission_and_is_idempotent() {
    let mut p = plan(2, false, false);
    freeze(&mut p, 100.0);
    let mut router = FakeRouter::new(&p);
    let event = envelope(EnvelopePayload::DebatePreflight(p.preflight.clone()));
    assert!(
        !preflight::emit(&mut router, event.clone(), &p)
            .unwrap()
            .duplicate
    );
    assert!(preflight::emit(&mut router, event, &p).unwrap().duplicate);
    assert_eq!(router.preflights.len(), 1);
    assert_eq!(router.state.state, DebateStatus::WaitingForDebaters);
    assert!(router.work.is_empty());
}
#[test]
fn stale_transition_cannot_allocate_or_change_state() {
    let p = plan(2, false, false);
    let mut router = FakeRouter::new(&p);
    let state = router.state.clone();
    let id = EventId::new();
    machine::submit(&mut router, id, &state, DebateTransition::PlanReady).unwrap();
    assert!(
        machine::submit(&mut router, id, &state, DebateTransition::PlanReady)
            .unwrap()
            .duplicate
    );
    assert_code(
        machine::submit(
            &mut router,
            EventId::new(),
            &state,
            DebateTransition::PlanReady,
        ),
        ErrorCode::INVALID_STATE_TRANSITION,
    );
    assert_eq!(
        router
            .debate_state(p.preflight.debate_id)
            .unwrap()
            .state_version,
        state.state_version + 1
    );
}
#[test]
fn bounds_and_daily_max_refuse_new_dispatch_only() {
    let mut p = plan(2, false, true);
    p.judge.as_mut().unwrap().context_tokens = 100;
    assert_eq!(
        preflight::freeze(
            &mut p,
            preflight::ContextBounds {
                members: [100, 100],
                judge: 100
            },
            100.0,
            100.0,
            now()
        ),
        Err(ErrorCode::CONTEXT_TOO_LARGE)
    );
    let mut p = plan(2, false, true);
    assert_eq!(
        preflight::freeze(
            &mut p,
            preflight::ContextBounds {
                members: [100, 100],
                judge: 100
            },
            0.0,
            100.0,
            now()
        ),
        Err(ErrorCode::DAILY_MAX_REACHED)
    );
    assert_eq!(
        admission::dispatch_allowed(20.0, 20.0, false),
        Err(ErrorCode::DAILY_MAX_REACHED)
    );
    assert_eq!(admission::dispatch_allowed(21.0, 20.0, true), Ok(()));
    assert_eq!(
        admission::dispatch_allowed(f64::NAN, 20.0, false),
        Err(ErrorCode::CONFIG_INVALID)
    );
}
#[test]
fn all_kinds_share_one_claim() {
    let id = ContestId::new();
    for kind in [
        ContestKind::Debate,
        ContestKind::Tournament,
        ContestKind::Grill,
        ContestKind::Census,
        ContestKind::Crosscheck,
        ContestKind::Survey,
        ContestKind::Probe,
    ] {
        assert_eq!(
            contests::check_claim(Some((id, kind)), ContestId::new()),
            Err(format!(
                "A {} is already running. Stop it before starting another.",
                kind.as_str()
            ))
        );
        assert!(contests::check_claim(Some((id, kind)), id).is_ok());
    }
}
#[test]
fn earlier_queues_drain_and_waiting_has_no_provider_deadline() {
    let p = plan(1, false, false);
    let s = state(&p, DebateStatus::WaitingForDebaters);
    assert_eq!(admission::ready_transition(&s, [true, false]), Ok(None));
    assert_eq!(
        admission::ready_transition(&s, [false, false]),
        Ok(Some(DebateTransition::DebatersReady))
    );
    assert!(admission::held_for_contest(&s));
    assert!(!admission::held_for_contest(&state(
        &p,
        DebateStatus::AwaitingConfirmation
    )));
}
#[test]
fn invalid_round_bounds_are_rejected() {
    for rounds in [0, 21] {
        let mut p = plan(rounds, false, false);
        assert!(
            preflight::freeze(
                &mut p,
                preflight::ContextBounds {
                    members: [1, 1],
                    judge: 1
                },
                100.0,
                100.0,
                now()
            )
            .is_err()
        );
    }
}

#[test]
fn frozen_plan_roundtrips_and_lifecycle_completes_judged_or_unjudged() {
    for judged in [false, true] {
        let mut p = plan(2, true, judged);
        freeze(&mut p, 100.0);
        let value = squad_core::orchestrator::provenance::encode_plan(&p).unwrap();
        let restored: squad_core::orchestrator::types::DebatePlan =
            serde_json::from_value(value).unwrap();
        assert_eq!(restored, p);
        let mut s = state(&p, DebateStatus::Preparing);
        for t in [
            DebateTransition::PlanReady,
            DebateTransition::Confirm,
            DebateTransition::DebatersReady,
        ] {
            s = machine::advance(&s, &t, true).unwrap();
        }
        assert_eq!(s.round, 0);
        s = machine::advance(&s, &DebateTransition::RoundResolved, true).unwrap();
        s = machine::advance(&s, &DebateTransition::OpeningBarrierCommitted, true).unwrap();
        assert_eq!(s.round, 1);
        s = machine::advance(&s, &DebateTransition::RoundResolved, true).unwrap();
        s = machine::advance(&s, &DebateTransition::NumberedBarrierCommitted, true).unwrap();
        assert_eq!(s.round, 2);
        s = machine::advance(&s, &DebateTransition::RoundResolved, true).unwrap();
        s = machine::advance(
            &s,
            &if judged {
                DebateTransition::JudgedFinalBarrierCommitted
            } else {
                DebateTransition::UnjudgedFinalBarrierCommitted
            },
            true,
        )
        .unwrap();
        if judged {
            s = machine::advance(
                &s,
                &DebateTransition::VerdictAccepted {
                    verdict_event_id: EventId::new(),
                },
                true,
            )
            .unwrap();
            assert_eq!(s.state, DebateStatus::Completed);
        } else {
            assert_eq!(s.state, DebateStatus::CompleteUnjudged);
        }
        assert!(!admission::held_for_contest(&s));
    }
}

#[test]
fn round_reasoning_is_low_even_when_final_reasoning_is_high() {
    let mut p = plan(1, false, true);
    let judge = p.judge.as_mut().unwrap();
    judge.reasoning_effort = Some(ReasoningEffort::High);
    judge.reasoning_tokens = constants::REASONING_HIGH_TOKENS;
    freeze(&mut p, 100.0);
    let round = p
        .calls
        .iter()
        .find(|c| c.kind == WorkKind::JudgeRound)
        .unwrap();
    assert_eq!(round.reasoning_effort, Some(ReasoningEffort::Low));
    assert_eq!(round.reasoning_tokens + round.visible_tokens, 1536);
    let final_call = p.calls.last().unwrap();
    assert_eq!(final_call.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(
        final_call.reasoning_tokens + final_call.visible_tokens,
        12288
    );
    let mut p = plan(1, false, true);
    let judge = p.judge.as_mut().unwrap();
    judge.reasoning_effort = None;
    judge.reasoning_tokens = 0;
    freeze(&mut p, 100.0);
    assert!(
        p.calls
            .iter()
            .filter(|c| c.actor == Target::Judge)
            .all(|c| c.reasoning_effort.is_none() && c.reasoning_tokens == 0)
    );
}
