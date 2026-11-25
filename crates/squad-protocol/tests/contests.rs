mod common;
use common::*;
use serde_json::json;
use squad_protocol::*;
#[test]
fn selection_state_and_generic_preflights_have_one_shared_shape() {
    let selection = json!({"contestId":ID,"kind":"debate","state":"AWAITING_PICK","stateVersion":3,"questionMessageId":ID,"addressed":["a","b"],"answers":[{"nick":"a","messageId":ID,"text":"answer"}],"requiredPicks":2});
    let v = envelope(
        "contest.state",
        json!({"kind":"selection","state":selection}),
    );
    assert!(ProducerEnvelope::from_value(&v).is_ok());
    let mut v = v;
    v["payload"]["state"]["answers"][0]["nick"] = json!("foreign");
    assert!(ProducerEnvelope::from_value(&v).is_err());
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
            kind.is_built(),
            matches!(kind, ContestKind::Debate | ContestKind::Census)
        );
    }
    let v = json!({"contestId":ID,"kind":"debate","expectedStateVersion":3,"transition":{"kind":"debate","transition":{"kind":"round_score_accepted","scoreEventId":ID}}});
    let submission: ContestTransitionSubmission = validation::decode(&v).unwrap();
    assert_eq!(submission.kind, ContestKind::Debate);
}
#[test]
fn parsed_inputs_map_to_atomic_requests() {
    let parsed = ParsedInput::Assignment {
        raw: "ROUNDS=3 GRILL_ROUNDS=4".into(),
        updates: vec![
            VariableUpdate {
                name: VariableName::Rounds,
                value: 3,
            },
            VariableUpdate {
                name: VariableName::GrillRounds,
                value: 4,
            },
        ],
    };
    let request =
        Request::from_parsed(ID.parse().unwrap(), parsed, &Default::default(), false).unwrap();
    assert!(matches!(request,Request::SetVars{updates,..} if updates.len()==2));
}
#[test]
fn preflight_call_counts_expiry_and_daily_projection_are_exact() {
    let p:DebatePreflight=validation::decode(&json!({"debateId":ID,"planId":ID,"expiresAt":TIME,"estimate":{"estimatedUsd":1.0,"reservedUsd":2.0,"remainingUsd":20.0,"modelCalls":12},"judgeOnly":false,"confirmationRequired":true,"debaters":["a","b"],"rounds":3,"judgeRequested":true,"openingRoundRequired":true})).unwrap();
    assert_eq!(p.planned_calls(), 12);
    assert_eq!(p.reserved_calls(), 25);
    assert!(p.validate().is_ok());
    assert!(
        p.validate_confirmation(
            ID.parse().unwrap(),
            ID.parse().unwrap(),
            "2026-11-24T12:01:00Z".parse().unwrap(),
            false
        )
        .is_err()
    );
    assert!(!valid_day("2026-02-30"));
    assert!(!valid_day("2026-2-01"));
    assert!(valid_day("2026-11-24"));
}
#[test]
fn reader_and_bus_contracts_are_object_safe() {
    fn contracts(
        _: Option<&dyn Bus>,
        _: Option<&dyn HistoryReader>,
        _: Option<&dyn WorkReader>,
        _: Option<&dyn ExecutorSubmit>,
        _: Option<&dyn PromptRenderer>,
    ) {
    }
    contracts(None, None, None, None, None);
}
#[test]
fn every_debate_transition_uses_its_declared_discriminant_and_reference_fields() {
    let variants = [
        "prepare",
        "plan_ready",
        "plan_rejected",
        "confirm",
        "cancel",
        "expire",
        "wait_for_debaters",
        "debaters_ready",
        "round_entered",
        "slot_resolved",
        "round_resolved",
        "opening_barrier_committed",
        "numbered_barrier_committed",
        "judged_final_barrier_committed",
        "unjudged_final_barrier_committed",
        "verdict_accepted",
        "final_judging_failed",
        "stop_requested",
        "stop_committed",
        "unrecoverable_failure",
        "round_score_accepted",
        "round_score_exhausted",
        "round_scoring_finished",
        "judge_only_requested",
        "judge_only_plan_ready",
        "judge_only_plan_rejected",
        "judge_only_confirmed",
        "judge_only_cancelled",
        "judge_only_expired",
    ];
    for kind in variants {
        let mut value = json!({"kind":kind});
        let reference = match kind {
            "slot_resolved" => Some("outcomeEventId"),
            "verdict_accepted" => Some("verdictEventId"),
            "round_score_accepted" => Some("scoreEventId"),
            "round_score_exhausted" => Some("workId"),
            _ => None,
        };
        if let Some(field) = reference {
            value[field] = json!(ID);
        }
        let transition: DebateTransition =
            validation::decode(&value).unwrap_or_else(|e| panic!("{kind}: {e}"));
        assert_eq!(serde_json::to_value(transition).unwrap(), value);
        value["extra"] = json!(true);
        assert!(validation::decode::<DebateTransition>(&value).is_err());
    }
}
#[test]
fn anonymous_history_has_no_attribution_or_false_anonymity() {
    assert!(validation::decode::<HistoryMetadata>(&json!({"anonymous":true})).is_ok());
    assert!(validation::decode::<HistoryMetadata>(&json!({"anonymous":false})).is_err());
    assert!(
        validation::decode::<HistoryMetadata>(&json!({"anonymous":true,"originEventId":ID}))
            .is_err()
    );
    let meta = json!({"originEventId":ID,"originSeq":"1","author":"a","projectionKey":"self"});
    assert!(validation::decode::<HistoryMetadata>(&meta).is_ok());
}
#[test]
fn request_serialization_preserves_variant_and_field_spelling() {
    let request = Request::Stop {
        request_id: ID.parse().unwrap(),
        target: None,
    };
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({"Stop":{"requestId":ID}})
    );
    let request = Request::ContestPick {
        request_id: ID.parse().unwrap(),
        contest_id: ID.parse().unwrap(),
        nicks: vec![Nick::new("a").unwrap(), Nick::new("b").unwrap()],
    };
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({"ContestPick":{"requestId":ID,"contestId":ID,"nicks":["a","b"]}})
    );
}

#[test]
fn shared_parser_api_maps_modes_picks_and_all_export_scopes() {
    use squad_protocol::parser::{
        ParseContext, PendingPick, QuestionFirstMode, SubmissionMeta, parse,
    };
    let mut ctx = ParseContext {
        roster: ["a", "b", "c", "d"].map(|n| Nick::new(n).unwrap()).to_vec(),
        ..Default::default()
    };
    let meta = SubmissionMeta::default();
    for raw in [
        "@a @b $debate Fresh topic",
        "@all $debate Pick after answers",
        "@all $tournament Design a CLI",
        "@all $grill Design a plan",
        "@all $census Pasted plan",
        "@all $crosscheck Pasted plan",
        "@all $survey Pick an option",
        "@all $probe Inspect logs; database ruled out",
    ] {
        let parsed = parse(raw, &meta, &ctx).unwrap();
        let expected = parsed.clone();
        let req = Request::from_parsed_with_context(ID.parse().unwrap(), parsed, &meta, &ctx, true)
            .unwrap();
        assert!(
            matches!(req, Request::UserMessage { parsed, input, preconfirmed:true } if parsed == expected && input.text.as_str() == raw)
        );
    }
    ctx.pending_pick = Some(PendingPick {
        contest_id: ID.parse().unwrap(),
        mode: QuestionFirstMode::Debate,
        eligible: ctx.roster.clone(),
    });
    let parsed = parse("@a @b", &meta, &ctx).unwrap();
    assert!(
        matches!(Request::from_parsed_with_context(ID.parse().unwrap(), parsed, &meta, &ctx, false).unwrap(), Request::ContestPick { nicks, .. } if nicks.len() == 2)
    );
    for scope in [
        "room",
        "debate",
        "grill",
        "tournament",
        "census",
        "crosscheck",
        "survey",
        "probe",
    ] {
        let parsed = parse(&format!("/export {scope}"), &meta, &ctx).unwrap();
        assert!(matches!(
            Request::from_parsed_with_context(ID.parse().unwrap(), parsed, &meta, &ctx, false)
                .unwrap(),
            Request::Export { id: None, .. }
        ));
    }
}
