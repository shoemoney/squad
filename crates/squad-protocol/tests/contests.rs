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
            matches!(
                kind,
                ContestKind::Debate
                    | ContestKind::Grill
                    | ContestKind::Tournament
                    | ContestKind::Census
                    | ContestKind::Crosscheck
                    | ContestKind::Probe
            ) || (cfg!(feature = "mode-survey") && kind == ContestKind::Survey)
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

#[test]
fn nine_mode_registries_roundtrip_and_remain_disabled() {
    assert!(!ContestKind::Forge.is_built());
    assert_eq!(serde_json::to_value(ContestKind::Forge).unwrap(), "forge");
    let state = ForgeState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: ForgeStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    state.validate().unwrap();
    let state = ContestState::Forge(state);
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["kind"], "forge");
    assert_eq!(serde_json::from_value::<ContestState>(json).unwrap(), state);
    assert!(!ContestKind::Forecast.is_built());
    assert_eq!(
        serde_json::to_value(ContestKind::Forecast).unwrap(),
        "forecast"
    );
    let state = ForecastState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: ForecastStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    state.validate().unwrap();
    let state = ContestState::Forecast(state);
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["kind"], "forecast");
    assert_eq!(serde_json::from_value::<ContestState>(json).unwrap(), state);
    assert!(!ContestKind::Decide.is_built());
    assert_eq!(serde_json::to_value(ContestKind::Decide).unwrap(), "decide");
    let state = DecideState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: DecideStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    state.validate().unwrap();
    let state = ContestState::Decide(state);
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["kind"], "decide");
    assert_eq!(serde_json::from_value::<ContestState>(json).unwrap(), state);
    assert!(!ContestKind::Negotiate.is_built());
    assert_eq!(
        serde_json::to_value(ContestKind::Negotiate).unwrap(),
        "negotiate"
    );
    let state = NegotiateState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: NegotiateStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    state.validate().unwrap();
    let state = ContestState::Negotiate(state);
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["kind"], "negotiate");
    assert_eq!(serde_json::from_value::<ContestState>(json).unwrap(), state);
    assert!(ContestKind::BlowItUp.is_built());
    assert_eq!(
        serde_json::to_value(ContestKind::BlowItUp).unwrap(),
        "blow_it_up"
    );
    let state = BlowItUpState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: BlowItUpStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    state.validate().unwrap();
    let state = ContestState::BlowItUp(state);
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["kind"], "blow_it_up");
    assert_eq!(serde_json::from_value::<ContestState>(json).unwrap(), state);
    assert!(!ContestKind::Readback.is_built());
    assert_eq!(
        serde_json::to_value(ContestKind::Readback).unwrap(),
        "readback"
    );
    let state = ReadbackState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: ReadbackStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    state.validate().unwrap();
    let state = ContestState::Readback(state);
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["kind"], "readback");
    assert_eq!(serde_json::from_value::<ContestState>(json).unwrap(), state);
    assert!(!ContestKind::Debugging.is_built());
    assert_eq!(
        serde_json::to_value(ContestKind::Debugging).unwrap(),
        "debug"
    );
    let state = DebuggingState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: DebuggingStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    state.validate().unwrap();
    let state = ContestState::Debugging(state);
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["kind"], "debug");
    assert_eq!(serde_json::from_value::<ContestState>(json).unwrap(), state);
    assert!(!ContestKind::Trace.is_built());
    assert_eq!(serde_json::to_value(ContestKind::Trace).unwrap(), "trace");
    let state = TraceState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: TraceStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    state.validate().unwrap();
    let state = ContestState::Trace(state);
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["kind"], "trace");
    assert_eq!(serde_json::from_value::<ContestState>(json).unwrap(), state);
    assert!(!ContestKind::Rank.is_built());
    assert_eq!(serde_json::to_value(ContestKind::Rank).unwrap(), "rank");
    let state = RankState {
        contest_id: ContestId::new(),
        state_version: 1,
        state: RankStatus::Preflight,
        step_id: None,
        round: 0,
        artifact_event_ids: vec![],
        terminal_reason: None,
    };
    state.validate().unwrap();
    let state = ContestState::Rank(state);
    let json = serde_json::to_value(&state).unwrap();
    assert_eq!(json["kind"], "rank");
    assert_eq!(serde_json::from_value::<ContestState>(json).unwrap(), state);
}
#[test]
fn all_new_work_kinds_have_the_frozen_composition_and_attempt_policy() {
    let kind = WorkKind::ForgeDraft;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "forge.draft");
    assert_eq!(kind.new_mode_composition(), Some("forge.draft"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::ForgeRank;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "forge.rank");
    assert_eq!(kind.new_mode_composition(), Some("forge.rank"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::ForgeFuse;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "forge.fuse");
    assert_eq!(kind.new_mode_composition(), Some("forge.fuse"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::ForgeJudge;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "forge.judge");
    assert_eq!(kind.new_mode_composition(), Some("forge.judge"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::ForecastFrame;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "forecast.frame");
    assert_eq!(kind.new_mode_composition(), Some("forecast.frame"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::ForecastEstimate;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "forecast.estimate");
    assert_eq!(kind.new_mode_composition(), Some("forecast.estimate"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::ForecastRevise;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "forecast.revise");
    assert_eq!(kind.new_mode_composition(), Some("forecast.revise"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::ForecastCruxes;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "forecast.cruxes");
    assert_eq!(kind.new_mode_composition(), Some("forecast.cruxes"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::DecideOptions;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "decide.options");
    assert_eq!(kind.new_mode_composition(), Some("decide.options"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::DecideShortlist;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "decide.shortlist");
    assert_eq!(kind.new_mode_composition(), Some("decide.shortlist"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::DecideCriteria;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "decide.criteria");
    assert_eq!(kind.new_mode_composition(), Some("decide.criteria"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::DecideFreeze;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "decide.freeze");
    assert_eq!(kind.new_mode_composition(), Some("decide.freeze"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::DecideScore;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "decide.score");
    assert_eq!(kind.new_mode_composition(), Some("decide.score"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::DecideExplain;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "decide.explain");
    assert_eq!(kind.new_mode_composition(), Some("decide.explain"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::NegotiateMap;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "negotiate.map");
    assert_eq!(kind.new_mode_composition(), Some("negotiate.map"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::NegotiateBrief;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "negotiate.brief");
    assert_eq!(kind.new_mode_composition(), Some("negotiate.brief"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::NegotiatePackage;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "negotiate.package");
    assert_eq!(kind.new_mode_composition(), Some("negotiate.package"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::NegotiateMove;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "negotiate.move");
    assert_eq!(kind.new_mode_composition(), Some("negotiate.move"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::NegotiateCounter;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "negotiate.counter");
    assert_eq!(kind.new_mode_composition(), Some("negotiate.counter"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::NegotiatePlaybook;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "negotiate.playbook");
    assert_eq!(kind.new_mode_composition(), Some("negotiate.playbook"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::BlowItUpAttack;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "blow_it_up.attack");
    assert_eq!(kind.new_mode_composition(), Some("blow_it_up.attack"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::BlowItUpChallenge;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "blow_it_up.challenge");
    assert_eq!(kind.new_mode_composition(), Some("blow_it_up.challenge"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::BlowItUpRule;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "blow_it_up.rule");
    assert_eq!(kind.new_mode_composition(), Some("blow_it_up.rule"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::ReadbackInterpret;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "readback.interpret");
    assert_eq!(kind.new_mode_composition(), Some("readback.interpret"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::ReadbackDiff;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "readback.diff");
    assert_eq!(kind.new_mode_composition(), Some("readback.diff"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::ReadbackRewrite;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "readback.rewrite");
    assert_eq!(kind.new_mode_composition(), Some("readback.rewrite"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::DebuggingHypothesize;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "debug.hypothesize");
    assert_eq!(kind.new_mode_composition(), Some("debug.hypothesize"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::DebuggingMerge;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "debug.merge");
    assert_eq!(kind.new_mode_composition(), Some("debug.merge"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::DebuggingTest;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "debug.test");
    assert_eq!(kind.new_mode_composition(), Some("debug.test"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::DebuggingSelect;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "debug.select");
    assert_eq!(kind.new_mode_composition(), Some("debug.select"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::DebuggingReport;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "debug.report");
    assert_eq!(kind.new_mode_composition(), Some("debug.report"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::TraceExtract;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "trace.extract");
    assert_eq!(kind.new_mode_composition(), Some("trace.extract"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::TraceMerge;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "trace.merge");
    assert_eq!(kind.new_mode_composition(), Some("trace.merge"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::TraceVerify;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "trace.verify");
    assert_eq!(kind.new_mode_composition(), Some("trace.verify"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::TraceAssemble;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "trace.assemble");
    assert_eq!(kind.new_mode_composition(), Some("trace.assemble"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
    let kind = WorkKind::RankCompare;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "rank.compare");
    assert_eq!(kind.new_mode_composition(), Some("rank.compare"));
    assert_eq!(constants::attempt_limit(&kind), 2);
    assert!(!kind.new_mode_judge());
    let kind = WorkKind::RankExplain;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "rank.explain");
    assert_eq!(kind.new_mode_composition(), Some("rank.explain"));
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert!(kind.new_mode_judge());
}
#[test]
fn artifact_headers_require_explicit_nonempty_distinct_member_audiences() {
    let mut a = ContestArtifact {
        contest_id: ContestId::new(),
        step_id: StepId::new(),
        kind: Some(ContestKind::Forge),
        round: Some(0),
        actor: Some(Actor::System),
        audience: Some(ArtifactAudience::Human),
        body: Some(serde_json::json!({})),
        artifact_kind: Text::new("forge.draft").unwrap(),
        work_id: None,
        source_refs: vec![],
        data: serde_json::json!({}),
    };
    a.validate_header().unwrap();
    assert!(a.validate_mode().is_err());
    a.audience = Some(ArtifactAudience::Members { recipients: vec![] });
    assert!(a.validate_header().is_err());
    let n: Nick = "a".parse().unwrap();
    a.audience = Some(ArtifactAudience::Members {
        recipients: vec![n.clone(), n.clone()],
    });
    assert!(a.validate_header().is_err());
    a.audience = Some(ArtifactAudience::Members {
        recipients: vec![n],
    });
    a.validate_header().unwrap();
    a.body = Some(serde_json::json!([]));
    assert!(a.validate_header().is_err());
}

#[test]
fn m9_work_classes_and_explicit_wire_names_are_registered() {
    let kind = WorkKind::TuneCases;
    let judge = false;
    let executor = false;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "tune.cases");
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 2);
    let kind = WorkKind::TuneFreeze;
    let judge = true;
    let executor = false;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "tune.freeze");
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 3);
    let kind = WorkKind::TuneExecute;
    let judge = false;
    let executor = true;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "tune.execute");
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 2);
    let kind = WorkKind::TuneScore;
    let judge = true;
    let executor = false;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "tune.score");
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 3);
    let kind = WorkKind::TuneMutate;
    let judge = false;
    let executor = false;
    assert_eq!(serde_json::to_value(&kind).unwrap(), "tune.mutate");
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 2);
    let kind = WorkKind::BassAckwardsPrdDone;
    let judge = false;
    let executor = false;
    assert_eq!(
        serde_json::to_value(&kind).unwrap(),
        "bass_ackwards_prd.done"
    );
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 2);
    let kind = WorkKind::BassAckwardsPrdFreeze;
    let judge = true;
    let executor = false;
    assert_eq!(
        serde_json::to_value(&kind).unwrap(),
        "bass_ackwards_prd.freeze"
    );
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 3);
    let kind = WorkKind::BassAckwardsPrdTree;
    let judge = false;
    let executor = false;
    assert_eq!(
        serde_json::to_value(&kind).unwrap(),
        "bass_ackwards_prd.tree"
    );
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 2);
    let kind = WorkKind::BassAckwardsPrdMerge;
    let judge = true;
    let executor = false;
    assert_eq!(
        serde_json::to_value(&kind).unwrap(),
        "bass_ackwards_prd.merge"
    );
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 3);
    let kind = WorkKind::BassAckwardsPrdReview;
    let judge = false;
    let executor = false;
    assert_eq!(
        serde_json::to_value(&kind).unwrap(),
        "bass_ackwards_prd.review"
    );
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 2);
    let kind = WorkKind::BassAckwardsPrdRevise;
    let judge = true;
    let executor = false;
    assert_eq!(
        serde_json::to_value(&kind).unwrap(),
        "bass_ackwards_prd.revise"
    );
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 3);
    let kind = WorkKind::BassAckwardsPrdMustHave;
    let judge = false;
    let executor = false;
    assert_eq!(
        serde_json::to_value(&kind).unwrap(),
        "bass_ackwards_prd.must_have"
    );
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 2);
    let kind = WorkKind::BassAckwardsPrdFinalize;
    let judge = true;
    let executor = false;
    assert_eq!(
        serde_json::to_value(&kind).unwrap(),
        "bass_ackwards_prd.finalize"
    );
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 3);
    let kind = WorkKind::BassAckwardsPrdApply;
    let judge = true;
    let executor = false;
    assert_eq!(
        serde_json::to_value(&kind).unwrap(),
        "bass_ackwards_prd.apply"
    );
    assert_eq!(kind.new_mode_judge(), judge);
    assert_eq!(kind.is_executor(), executor);
    assert_eq!(constants::attempt_limit(&kind), 3);
    assert_eq!(
        serde_json::to_value(ContestKind::BassAckwardsPrd).unwrap(),
        "bass_ackwards_prd"
    );
    assert_eq!(
        serde_json::to_value(ExportScope::BassAckwardsPrd).unwrap(),
        "bass_ackwards_prd"
    );
    assert_eq!(WorkKind::TuneExecute.new_mode_composition(), None);
    assert_eq!(WorkKind::TuneExecute.new_mode_visible_tokens(), Some(1024));
    assert_eq!(
        WorkKind::BassAckwardsPrdMerge.new_mode_visible_tokens(),
        Some(8192)
    );
}
