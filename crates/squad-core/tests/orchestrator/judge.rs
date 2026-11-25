use super::support::*;
use serde_json::{Value, json};
use squad_core::orchestrator::{
    cases,
    judge::{self, JudgeResult},
    preflight,
    types::*,
};
use squad_protocol::*;
use std::collections::BTreeMap;

fn scores(a: u8, b: u8) -> Value {
    json!({"scores":{"alpha":{"score":a,"reason":"Clear argument."},"beta":{"score":b,"reason":"Supported answer."}}})
}
fn verdict_value(winner: &str, override_reason: Value) -> Value {
    json!({"winner":winner,"scores":[{"nick":"alpha","criteria":[{"criterionId":"fit","score":8,"reasoning":"Fits."},{"criterionId":"feasibility","score":8,"reasoning":"Feasible."},{"criterionId":"originality","score":8,"reasoning":"Original."},{"criterionId":"survivedAttacks","score":8,"reasoning":"Defended."}]},{"nick":"beta","criteria":[{"criterionId":"fit","score":8,"reasoning":"Fits."},{"criterionId":"feasibility","score":8,"reasoning":"Feasible."},{"criterionId":"originality","score":8,"reasoning":"Original."},{"criterionId":"survivedAttacks","score":8,"reasoning":"Defended."}]}],"reasoning":"Instruction decides between equal totals.","whatToStealFromLoser":[],"tallyOverrideReason":override_reason})
}
pub fn record(s: &DebateState, round: u32, a: u8, b: u8) -> DebateRoundScore {
    DebateRoundScore {
        debate_id: s.debate_id,
        case_id: s.case_id.unwrap(),
        round,
        scores: Some(
            verdict::validate_round_response(&scores(a, b), &s.debaters)
                .unwrap()
                .scores,
        ),
        tally: BTreeMap::from([(nick("alpha"), 0), (nick("beta"), 0)]),
        reason: None,
    }
}
#[test]
fn model_conflicts_use_concrete_slugs_and_stable_member_order() {
    let members = [model("alpha"), model("beta")];
    let mut default = model("renamed_fable");
    default.resolved_slug = members[0].resolved_slug.clone();
    let mut duplicate = model("other_nick");
    duplicate.resolved_slug = members[1].resolved_slug.clone();
    let (chosen, warning) = judge::select_model(
        &default,
        &[members[0].clone(), duplicate, model("referee")],
        &members,
    )
    .unwrap();
    assert_eq!(chosen.resolved_slug, "vendor/referee");
    assert_eq!(
        warning.as_deref(),
        Some("Judge model changed to vendor/referee because @alpha is debating.")
    );
    assert!(judge::select_model(&default, &[], &members).is_err());
    assert!(
        judge::select_model(&model("referee"), &[], &members)
            .unwrap()
            .1
            .is_none()
    );
}
#[test]
fn equal_round_scores_are_accepted_strict_invalid_shapes_repair_once_then_exhaust() {
    let p = plan(1, false, true);
    let mut s = state(&p, DebateStatus::RoundRunning);
    case(&mut s);
    let a = attempt(judge_work(&p, &s, Some(1)), AttemptPhase::Running);
    assert!(matches!(
        judge::round_result(&s, &a, &scores(8, 8).to_string(), now()).unwrap(),
        JudgeResult::Accepted(_)
    ));
    let mut invalid = vec![
        json!({}),
        json!({"scores":{"alpha":{"score":8,"reason":"ok"}}}),
        scores(0, 10),
        scores(10, 11),
        json!({"scores":{"alpha":{"score":1.5,"reason":"ok"},"beta":{"score":8,"reason":"ok"}}}),
    ];
    let mut extra = scores(8, 8);
    extra["tally"] = json!({});
    invalid.push(extra);
    let mut long = scores(8, 8);
    long["scores"]["alpha"]["reason"] = json!("x".repeat(201));
    invalid.push(long);
    for raw in invalid {
        assert!(matches!(
            judge::round_result(&s, &a, &raw.to_string(), now()).unwrap(),
            JudgeResult::Repair(JudgeRepairData { attempt: 2, .. })
        ));
    }
    let mut second = a.clone();
    second.work.attempt = 2;
    second.repair_used = true;
    assert_eq!(
        judge::round_result(&s, &second, "not json", now()).unwrap(),
        JudgeResult::Exhausted(ErrorCode::JUDGE_INVALID_VERDICT)
    );
    s.state = DebateStatus::Stopping;
    assert_eq!(
        judge::round_result(&s, &a, &scores(8, 8).to_string(), now()),
        Err(ErrorCode::WORK_CANCELLED)
    );
}
#[test]
fn late_scores_sum_each_round_once_and_unscored_adds_nothing() {
    let p = plan(3, false, true);
    let mut s = state(&p, DebateStatus::Judging);
    case(&mut s);
    let a = record(&s, 2, 9, 4);
    let b = record(&s, 1, 6, 8);
    let unscored = DebateRoundScore {
        round: 3,
        scores: None,
        reason: Some(Text::new("unscored").unwrap()),
        ..a.clone()
    };
    let evidence =
        judge::scoring_evidence(&s, &[a.clone(), b, unscored, a.clone()], &[1, 2, 3]).unwrap();
    assert_eq!(evidence.round_tally[&nick("alpha")], 15);
    assert_eq!(evidence.round_tally[&nick("beta")], 12);
    assert_eq!(evidence.unscored_rounds, vec![3]);
    assert_eq!(evidence.round_scores.len(), 3);
    let different = record(&s, 2, 8, 4);
    assert_eq!(
        judge::scoring_evidence(&s, &[a, different], &[1, 2]),
        Err(ErrorCode::IDEMPOTENCY_CONFLICT)
    );
}
#[test]
fn final_waits_for_all_scores_and_does_not_allocate_twice() {
    let p = plan(2, false, true);
    let mut s = state(&p, DebateStatus::Judging);
    case(&mut s);
    let records = vec![record(&s, 1, 7, 8), record(&s, 2, 9, 8)];
    assert!(!judge::final_ready(
        &s,
        &[1, 2],
        &records[..1],
        &[],
        false,
        false
    ));
    assert!(judge::final_ready(&s, &[1, 2], &records, &[], false, false));
    assert!(!judge::final_ready(&s, &[1, 2], &records, &[], true, false));
    let pending = attempt(judge_work(&p, &s, Some(2)), AttemptPhase::Running);
    assert!(!judge::final_ready(
        &s,
        &[1, 2],
        &records,
        &[pending],
        false,
        false
    ));
    assert!(judge::final_ready(&s, &[1], &[], &[], false, true));
}
#[test]
fn app_totals_override_reason_and_required_nullable_use_protocol_validation() {
    let p = plan(2, false, true);
    let mut s = state(&p, DebateStatus::Judging);
    case(&mut s);
    let a = attempt(judge_work(&p, &s, None), AttemptPhase::Running);
    let tally = BTreeMap::from([(nick("alpha"), 10), (nick("beta"), 15)]);
    let raw = verdict_value("alpha", Value::Null);
    assert!(matches!(
        judge::final_result(&s, &a, &raw.to_string(), &tally, now()).unwrap(),
        JudgeResult::Repair(_)
    ));
    let raw = verdict_value("alpha", json!("Stronger fit to the user's instruction."));
    let JudgeResult::Accepted(v) =
        judge::final_result(&s, &a, &raw.to_string(), &tally, now()).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        v.weighted_totals
            .iter()
            .map(|v| v.total)
            .collect::<Vec<_>>(),
        vec![8.0, 8.0]
    );
    assert_eq!(v.round_tally, tally);
    let zero = BTreeMap::from([(nick("alpha"), 0), (nick("beta"), 0)]);
    assert!(matches!(
        judge::final_result(
            &s,
            &a,
            &verdict_value("beta", Value::Null).to_string(),
            &zero,
            now()
        )
        .unwrap(),
        JudgeResult::Accepted(_)
    ));
    for key in ["tallyOverrideReason", "winner"] {
        let mut bad = raw.clone();
        bad.as_object_mut().unwrap().remove(key);
        assert!(matches!(
            judge::final_result(&s, &a, &bad.to_string(), &tally, now()).unwrap(),
            JudgeResult::Repair(_)
        ));
    }
    let mut bad = raw.clone();
    bad["roundTally"] = json!({"alpha":1,"beta":0});
    assert!(matches!(
        judge::final_result(&s, &a, &bad.to_string(), &tally, now()).unwrap(),
        JudgeResult::Repair(_)
    ));
    let tied = BTreeMap::from([(nick("alpha"), 8), (nick("beta"), 8)]);
    assert!(matches!(
        judge::final_result(
            &s,
            &a,
            &verdict_value("beta", Value::Null).to_string(),
            &tied,
            now()
        )
        .unwrap(),
        JudgeResult::Accepted(_)
    ));
    let mut inconsistent = raw.clone();
    for criterion in inconsistent["scores"][1]["criteria"]
        .as_array_mut()
        .unwrap()
    {
        criterion["score"] = json!(9);
    }
    assert!(matches!(
        judge::final_result(&s, &a, &inconsistent.to_string(), &tally, now()).unwrap(),
        JudgeResult::Repair(_)
    ));
    let mut bad = raw;
    bad["scores"][1]["nick"] = json!("alpha");
    assert!(matches!(
        judge::final_result(&s, &a, &bad.to_string(), &tally, now()).unwrap(),
        JudgeResult::Repair(_)
    ));
}
#[test]
fn final_transport_and_schema_repair_share_three_attempts() {
    let p = plan(1, false, true);
    let mut s = state(&p, DebateStatus::Judging);
    case(&mut s);
    let mut a = attempt(judge_work(&p, &s, None), AttemptPhase::Failed);
    assert!(matches!(
        squad_core::orchestrator::rounds::attempt(&a, now(), false),
        AttemptDecision::Retry {
            next_attempt: 2,
            repair: false,
            ..
        }
    ));
    a.work.attempt = 2;
    let tally = BTreeMap::from([(nick("alpha"), 0), (nick("beta"), 0)]);
    assert!(matches!(
        judge::final_result(&s, &a, "{}", &tally, now()).unwrap(),
        JudgeResult::Repair(JudgeRepairData { attempt: 3, .. })
    ));
    a.work.attempt = 3;
    a.repair_used = true;
    assert_eq!(
        judge::final_result(&s, &a, "{}", &tally, now()).unwrap(),
        JudgeResult::Exhausted(ErrorCode::JUDGE_INVALID_VERDICT)
    );
}
#[test]
fn manual_preflight_has_one_call_no_round_scoring_or_new_member_calls() {
    let mut p = plan(3, false, true);
    p.preflight.judge_only = true;
    p.prior_terminal = Some(state(&p, DebateStatus::Stopped));
    preflight::freeze(
        &mut p,
        preflight::ContextBounds {
            members: [100, 100],
            judge: 3000,
        },
        100.0,
        100.0,
        now(),
    )
    .unwrap();
    assert_eq!(p.calls.len(), 1);
    assert_eq!(p.preflight.reserved_calls(), 3);
    assert_eq!(p.calls[0].kind, WorkKind::Judge);
    let mut s = state(&p, DebateStatus::Judging);
    case(&mut s);
    let w = judge_work(&p, &s, None);
    let mut round = w;
    round.kind = WorkKind::JudgeRound;
    round.round = Some(1);
    assert!(squad_core::orchestrator::rounds::prepare_judge(&p, round).is_err());
}
#[test]
fn case_bundle_and_notice_are_complete_and_case_only() {
    let p = plan(1, false, true);
    let mut s = state(&p, DebateStatus::Judging);
    case(&mut s);
    let setup = DebateSetup {
        debate_id: s.debate_id,
        topic_message_id: p.sources.topic_message_id,
        topic_text: p.sources.topic.clone(),
        solutions: p.sources.solutions.clone().unwrap(),
        instruction: p.sources.instruction.clone(),
        rounds: 1,
        judge_model: "vendor/referee".into(),
        judge_swapped: false,
    };
    let rubric = cases::canonical_rubric("v1").unwrap();
    let bundle = cases::bundle(&p, &s, &setup, &[], &[], &[], rubric).unwrap();
    assert_eq!(bundle.topic.text, p.sources.topic);
    assert!(bundle.transcript.is_empty());
    assert_eq!(bundle.round_scoring.round_tally[&nick("alpha")], 0);
    let opened = cases::debate(
        s.case_id.unwrap(),
        EventId::new(),
        Text::new("vendor/referee").unwrap(),
        setup,
        Text::new("v1").unwrap(),
    )
    .unwrap();
    let mut router = FakeRouter::new(&p);
    let event = envelope(EnvelopePayload::CaseOpened(opened));
    cases::open_debate(&mut router, event, bundle.clone()).unwrap();
    assert_eq!(router.state.case_id, s.case_id);
    let w = judge_work(&p, &s, None);
    let a = attempt(w.clone(), AttemptPhase::Running);
    let JudgeResult::Accepted(v) = judge::final_result(
        &s,
        &a,
        &verdict_value("alpha", Value::Null).to_string(),
        &bundle.round_scoring.round_tally,
        now(),
    )
    .unwrap() else {
        panic!()
    };
    let verdict = DebateVerdict {
        completed: CompletedGeneration {
            generation: GenerationIdentity {
                message_id: w.message_id,
                work_id: w.work_id,
                trigger_event_id: w.trigger_event_id,
                attempt: 1,
            },
            model: Text::new("vendor/referee").unwrap(),
            finish_reason: FinishReason::Stop,
            usage: None,
        },
        debate_id: s.debate_id,
        case_id: s.case_id.unwrap(),
        judge_invocation_id: EventId::new(),
        rubric_version: Text::new("v1").unwrap(),
        verdict: v,
    };
    let text = judge::verdict_notice(
        &s,
        &verdict,
        &bundle.round_scoring,
        &squad_core::prompts::PromptPack::embedded().unwrap(),
    )
    .unwrap();
    assert!(text.starts_with("[squad notice]"));
    assert!(text.contains("roundTally"));
    assert!(text.contains("@alpha wins"));
}

#[test]
fn manual_judge_retains_prior_case_scores_and_unscored_rounds() {
    let p = plan(2, false, true);
    let mut s = state(&p, DebateStatus::Stopped);
    case(&mut s);
    let old = record(&s, 1, 8, 6);
    let old_case = s.case_id.unwrap();
    case(&mut s);
    s.state = DebateStatus::Judging;
    let evidence = judge::scoring_evidence(&s, &[old], &[1, 2]).unwrap();
    assert_eq!(evidence.round_tally[&nick("alpha")], 8);
    assert_eq!(evidence.unscored_rounds, vec![2]);
    assert_eq!(evidence.round_scores[0].case_id, old_case);
    assert_ne!(s.case_id, Some(old_case));
}
#[test]
fn round_record_is_app_computed_and_exhaustion_is_explicit() {
    let p = plan(2, false, true);
    let mut s = state(&p, DebateStatus::Judging);
    case(&mut s);
    let response = verdict::validate_round_response(&scores(8, 7), &s.debaters).unwrap();
    let first = judge::round_record(&s, 1, Some(response.clone()), &[]).unwrap();
    assert_eq!(first.tally[&nick("alpha")], 8);
    assert_eq!(
        judge::round_record(&s, 1, Some(response), std::slice::from_ref(&first)).unwrap(),
        first
    );
    let second = judge::round_record(&s, 2, None, &[first]).unwrap();
    assert!(second.scores.is_none());
    assert_eq!(second.reason.unwrap().as_str(), "unscored");
    assert_eq!(second.tally[&nick("beta")], 7);
}
