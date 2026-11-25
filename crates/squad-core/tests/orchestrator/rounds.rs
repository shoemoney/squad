use super::support::*;
use squad_core::orchestrator::{
    contests::{self, EngineAction, ExecutionSnapshot},
    rounds,
    types::*,
};
use squad_protocol::*;

#[test]
fn frozen_pair_allocation_replays_without_new_calls() {
    let p = plan(2, false, true);
    let mut router = FakeRouter::new(&p);
    router.state = state(&p, DebateStatus::RoundRunning);
    let s = router.state.clone();
    let pair = [work(&p, 0, 1), work(&p, 1, 1)];
    let request = EventId::new();
    rounds::allocate_pair(&mut router, request, &s, &p, pair.clone()).unwrap();
    assert!(
        rounds::allocate_pair(&mut router, request, &s, &p, pair)
            .unwrap()
            .duplicate
    );
    assert_eq!(router.work.len(), 2);
    let bad = [work(&p, 0, 1), work(&p, 0, 1)];
    assert!(rounds::allocate_pair(&mut router, EventId::new(), &s, &p, bad).is_err());
}
#[test]
fn retry_preserves_identity_and_private_history_boundary() {
    let p = plan(2, false, true);
    let original = work(&p, 0, 1);
    let failed = attempt(original.clone(), AttemptPhase::Failed);
    let retry = rounds::retry(&failed, 2).unwrap();
    assert_eq!(retry.work_id, original.work_id);
    assert_eq!(retry.message_id, original.message_id);
    assert_eq!(retry.history_through, original.history_through);
    assert_eq!(retry.trigger_event_id, original.trigger_event_id);
    assert_eq!(
        rounds::retry(&failed, 3),
        Err(ErrorCode::STALE_WORK_ATTEMPT)
    );
    assert_eq!(
        rounds::prompt_values(&p, &retry).unwrap()["opponent"],
        "beta"
    );
}
#[test]
fn deadlines_second_attempt_and_forfeit_are_bounded() {
    let p = plan(1, true, false);
    let mut a = attempt(work(&p, 0, 0), AttemptPhase::Running);
    assert_eq!(rounds::attempt(&a, now(), false), AttemptDecision::Wait);
    let deadline = a.deadline.unwrap();
    assert!(matches!(
        rounds::attempt(&a, deadline, false),
        AttemptDecision::Retry {
            next_attempt: 2,
            ..
        }
    ));
    a.work.attempt = 2;
    assert_eq!(
        rounds::attempt(&a, deadline, false),
        AttemptDecision::Forfeit(a.work.work_id)
    );
    for reason in [ForfeitReason::Timeout, ForfeitReason::Error] {
        let DebateTurnOutcome::Forfeit { text, .. } = rounds::forfeit(reason) else {
            panic!()
        };
        assert!(text.as_str().starts_with("[squad notice]"));
    }
}
#[test]
fn final_compositions_and_word_limits_for_one_and_many_rounds() {
    for count in [1, 3] {
        let p = plan(count, true, true);
        let steps = contests::admitted_script(&p).unwrap();
        let fanouts: Vec<_> = steps
            .iter()
            .filter(|s| s.move_name == ContestMove::Fanout)
            .collect();
        assert_eq!(fanouts.len(), count as usize + 1);
        assert_eq!(fanouts[0].composition.as_deref(), Some("debate.round0"));
        assert_eq!(fanouts[0].word_limit, Some(200));
        let last = fanouts.last().unwrap();
        assert_eq!(last.word_limit, Some(400));
        assert!(
            last.composition
                .as_ref()
                .unwrap()
                .starts_with("debate.final.")
        );
        assert_eq!(last.actors.len(), 2);
        assert!(
            steps
                .iter()
                .filter(|s| matches!(
                    s.move_name,
                    ContestMove::Barrier | ContestMove::Tally | ContestMove::Report
                ))
                .all(|s| s.composition.is_none() && s.attempts == 0)
        );
    }
}
pub fn snapshot<'a>(
    state: &'a DebateState,
    slots: &'a [AttemptState],
    scoring: &'a [AttemptState],
    committed: &'a [u32],
    scores: &'a [DebateRoundScore],
) -> ExecutionSnapshot<'a> {
    ExecutionSnapshot {
        state,
        round_work: slots,
        scoring_work: scoring,
        committed_rounds: committed,
        scores,
        final_work_exists: false,
        manual_judge: false,
        earlier_unfinished: [false, false],
    }
}
#[test]
fn reverse_results_wait_for_both_and_do_not_block_later_rounds_on_scoring() {
    let p = plan(2, false, true);
    let mut s = state(&p, DebateStatus::RoundRunning);
    s.round = 2;
    let mut slots = [
        attempt(work(&p, 0, 2), AttemptPhase::Running),
        attempt(work(&p, 1, 2), AttemptPhase::Accepted(EventId::new())),
    ];
    assert_eq!(
        contests::next(&snapshot(&s, &slots, &[], &[1], &[]), now()),
        Ok(EngineAction::AllocateRoundScore(1))
    );
    slots[0].phase = AttemptPhase::Accepted(EventId::new());
    assert_eq!(
        contests::next(&snapshot(&s, &slots, &[], &[1], &[]), now()),
        Ok(EngineAction::Transition(DebateTransition::RoundResolved))
    );
    assert_eq!(
        contests::next(&snapshot(&s, &[], &[], &[1], &[]), now()),
        Ok(EngineAction::AllocateRound(2))
    );
}
#[test]
fn double_forfeit_requires_durable_outcomes_before_barrier() {
    let p = plan(1, false, false);
    let s = state(&p, DebateStatus::RoundRunning);
    let slots = [
        attempt(work(&p, 0, 1), AttemptPhase::Exhausted),
        attempt(work(&p, 1, 1), AttemptPhase::Exhausted),
    ];
    assert_eq!(
        contests::next(&snapshot(&s, &slots, &[], &[], &[]), now()),
        Ok(EngineAction::Attempt(AttemptDecision::Forfeit(
            slots[0].work.work_id
        )))
    );
    let resolved = slots.map(|mut a| {
        a.phase = AttemptPhase::Accepted(EventId::new());
        a
    });
    assert_eq!(
        contests::next(&snapshot(&s, &resolved, &[], &[], &[]), now()),
        Ok(EngineAction::Transition(DebateTransition::RoundResolved))
    );
}
#[test]
fn opening_setup_preserves_forfeits_and_finish_reasons() {
    let p = plan(1, true, false);
    let turns = [0, 1].map(|i| {
        let w = work(&p, i, 0);
        DebateTurn {
            generation: GenerationIdentity {
                message_id: w.message_id,
                work_id: w.work_id,
                trigger_event_id: w.trigger_event_id,
                attempt: 1,
            },
            debate_id: p.preflight.debate_id,
            round: 0,
            speaker: p.preflight.debaters[i].clone(),
            outcome: if i == 0 {
                rounds::forfeit(ForfeitReason::Error)
            } else {
                DebateTurnOutcome::Completed {
                    content: "Position B".into(),
                    model: Text::new("vendor/beta").unwrap(),
                    finish_reason: FinishReason::Length,
                    usage: None,
                }
            },
        }
    });
    let setup = rounds::opening_setup(&p, &turns).unwrap();
    assert!(setup.solutions[0].text.starts_with("[squad notice]"));
    assert_eq!(setup.solutions[1].text, "Position B");
    assert_eq!(
        setup.solutions[1].message_id,
        turns[1].generation.message_id
    );
}

fn committed_turn(p: &DebatePlan, index: usize, seq: i64) -> CommittedEnvelope {
    let w = work(p, index, 1);
    let turn = DebateTurn {
        generation: GenerationIdentity {
            message_id: w.message_id,
            work_id: w.work_id,
            trigger_event_id: w.trigger_event_id,
            attempt: 1,
        },
        debate_id: p.preflight.debate_id,
        round: 1,
        speaker: p.preflight.debaters[index].clone(),
        outcome: DebateTurnOutcome::Completed {
            content: format!("position {index}"),
            model: Text::new(format!("vendor/{index}")).unwrap(),
            finish_reason: FinishReason::ContentFilter,
            usage: None,
        },
    };
    let mut event = envelope(EnvelopePayload::DebateTurn(turn));
    event.base.sender = Actor::Member(p.preflight.debaters[index].clone());
    CommittedEnvelope {
        base: event.base,
        content: event.content,
        commit: CommitFields {
            seq: Cursor::new(seq).unwrap(),
            accepted_at: now(),
            commit_index: 0,
            commit_size: 1,
        },
    }
}
#[test]
fn barriers_preserve_stable_order_original_sources_roles_and_case_only_scores() {
    let p = plan(1, false, true);
    let mut s = state(&p, DebateStatus::RoundCommitting);
    case(&mut s);
    let a = committed_turn(&p, 0, 101);
    let b = committed_turn(&p, 1, 100);
    let projections = rounds::barrier_projections(&s, &[b, a.clone()]).unwrap();
    assert_eq!(projections.len(), 6);
    assert_eq!(projections[0].origin_event_id, a.base.id);
    assert_eq!(projections[0].context_id, ContextId("alpha".into()));
    assert_eq!(projections[0].role, HistoryRole::Assistant);
    assert_eq!(projections[1].role, HistoryRole::User);
    assert_eq!(projections[1].name.as_deref(), Some("beta"));
    assert_eq!(
        projections[0].meta,
        HistoryMetadata::Attributed {
            origin_event_id: a.base.id,
            origin_seq: a.commit.seq,
            author: a.base.sender,
            projection_key: Text::new("debate.turn").unwrap()
        }
    );
    assert!(projections[4].context_id.0.starts_with("judge:"));
    assert!(projections[4].content.contains("content_filter"));
    assert!(
        projections
            .iter()
            .all(|p| !p.content.contains("roundTally"))
    );
    let score = super::judge::record(&s, 1, 8, 8);
    let event = envelope(EnvelopePayload::DebateRoundScore(score));
    let committed = CommittedEnvelope {
        base: event.base,
        content: event.content,
        commit: CommitFields {
            seq: Cursor::new(200).unwrap(),
            accepted_at: now(),
            commit_index: 0,
            commit_size: 1,
        },
    };
    let score_projection =
        squad_core::orchestrator::judge::score_projection(&s, &committed).unwrap();
    assert!(score_projection.context_id.0.starts_with("judge:"));
    assert_eq!(score_projection.role, HistoryRole::Assistant);
}
#[test]
fn duplicate_barrier_is_one_delivery_and_allocates_no_extra_work() {
    let p = plan(1, false, true);
    let mut s = state(&p, DebateStatus::RoundCommitting);
    case(&mut s);
    let events = [committed_turn(&p, 0, 100), committed_turn(&p, 1, 101)];
    let step = StepId::new();
    let report = ContestReport {
        contest_id: s.debate_id,
        step_id: step,
        artifact_event_ids: events.iter().map(|e| e.base.id).collect(),
        skipped_work_ids: Vec::new(),
        data: serde_json::json!({"round":1}),
    };
    let barrier = BarrierCommit {
        contest_id: s.debate_id,
        step_id: step,
        expected_state_version: s.state_version,
        barrier_event: envelope(EnvelopePayload::ContestReport(report)),
        projections: rounds::barrier_projections(&s, &events).unwrap(),
        next_work: vec![judge_work(&p, &s, Some(1))],
    };
    let mut router = FakeRouter::new(&p);
    router.state = s;
    let id = EventId::new();
    contests::commit_barrier(&mut router, id, barrier.clone()).unwrap();
    assert!(
        contests::commit_barrier(&mut router, id, barrier)
            .unwrap()
            .duplicate
    );
    assert_eq!(router.deliveries, 6);
    assert_eq!(router.work.len(), 1);
}

#[test]
fn router_failure_policy_shares_budget_without_engine_requeue() {
    let id = WorkId::new();
    for kind in [WorkKind::DebateTurn, WorkKind::JudgeRound, WorkKind::Judge] {
        let limit = rounds::attempt_limit(&kind);
        for failure in [
            FailureClass::Transport,
            FailureClass::Timeout,
            FailureClass::Interrupted,
            FailureClass::InvalidOutput,
        ] {
            assert!(matches!(
                rounds::failure_policy(id, &kind, 1, limit, false, failure),
                AttemptDecision::Retry {
                    next_attempt: 2,
                    ..
                }
            ));
            assert!(!matches!(
                rounds::failure_policy(id, &kind, u32::from(limit), limit, false, failure),
                AttemptDecision::Retry { .. }
            ));
        }
    }
    assert_eq!(
        rounds::failure_policy(
            id,
            &WorkKind::Judge,
            2,
            3,
            true,
            FailureClass::InvalidOutput
        ),
        AttemptDecision::JudgeFailed(id)
    );
    assert!(matches!(
        rounds::failure_policy(id, &WorkKind::Judge, 2, 3, true, FailureClass::Transport),
        AttemptDecision::Retry {
            next_attempt: 3,
            repair: false,
            ..
        }
    ));
    let p = plan(1, false, true);
    let failed = attempt(work(&p, 0, 1), AttemptPhase::Failed);
    assert_eq!(rounds::engine_attempt(&failed), AttemptDecision::Wait);
}
