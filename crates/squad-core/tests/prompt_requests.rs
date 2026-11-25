use serde_json::{Value, json};
use squad_core::prompts::{PromptPack, inputs::*};
use squad_protocol::*;
use std::collections::BTreeMap;
const ID: &str = "01900000-0000-7000-8000-000000000001";
const OTHER: &str = "01900000-0000-7000-8000-000000000002";
fn config() -> SquadConfig {
    let dir = std::env::temp_dir().join(format!("squad-inputs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let options = config::LoadOptions {
        env_file: None,
        data_dir: Some(dir.clone()),
        working_dir: dir.clone(),
        config_dir: None,
        default_data_dir: dir,
    };
    SquadConfig::load_with_env(
        &options,
        &BTreeMap::from([
            ("OPENROUTER_API_KEY".into(), "test-fixture".into()),
            ("SQUAD_MODEL_ALPHA".into(), "vendor/alpha".into()),
            ("SQUAD_MODEL_BETA".into(), "vendor/beta".into()),
            ("SQUAD_PERSONA_ALPHA".into(), "Be precise {{rogue}}".into()),
            ("SQUAD_USER_NAME".into(), "Jeremy \"J\"".into()),
            ("SQUAD_TURN_WORDS".into(), "321".into()),
            ("SQUAD_FINAL_TURN_WORDS".into(), "543".into()),
        ]),
    )
    .unwrap()
}
fn binding() -> ExecutorBinding {
    ExecutorBinding {
        model: Text::new("vendor/concrete").unwrap(),
        effort: ReasoningEffort::High,
    }
}
fn work(kind: &str, nick: &str, round: u32) -> Work {
    let judge = kind.starts_with("judge");
    serde_json::from_value(json!({"workId":ID,"messageId":ID,"kind":kind,"target":if judge {"judge"} else {nick},"contextId":if judge {format!("judge:{ID}")} else {nick.into()},"triggerEventId":ID,"historyThrough":"10","attempt":1,"debateId":ID,"contestId":ID,"round":round,"caseId":if judge {json!(ID)} else {Value::Null},"maxOutputTokens":if kind=="judge.round" {512} else if judge {4096} else {2048}, "promptRef":{"templateId":if kind=="judge.round" {"judge.round.md"} else {"judge.debate"},"packVersion":"1.0.0","templateHash":"frozen"}})).unwrap()
}
fn work_values(nick: &str, rounds: u32) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "opponent".into(),
            if nick == "alpha" {
                "beta".into()
            } else {
                "alpha".into()
            },
        ),
        ("rounds".into(), rounds.to_string()),
    ])
}
fn opening(rounds: u32) -> DebateOpening {
    serde_json::from_value(json!({"debateId":ID,"debaters":["alpha","beta"],"topicMessageId":ID,"topicText":"UNIQUE_TOPIC","instruction":"UNIQUE_INSTRUCTION","rounds":rounds,"round":0})).unwrap()
}
fn setup(rounds: u32) -> DebateSetup {
    serde_json::from_value(json!({"debateId":ID,"topicMessageId":ID,"topicText":"UNIQUE_TOPIC","instruction":"UNIQUE_INSTRUCTION","rounds":rounds,"solutions":[{"nick":"alpha","messageId":ID,"text":"SOLUTION_A"},{"nick":"beta","messageId":OTHER,"text":"SOLUTION_B"}],"judgeModel":"vendor/judge","judgeSwapped":false})).unwrap()
}
fn bundle() -> DebateAdjudicationBundle {
    let transcript = (1..=3).flat_map(|round| ["alpha", "beta"].map(move |nick| json!({"messageId":ID,"workId":ID,"triggerEventId":ID,"attempt":1,"debateId":ID,"round":round,"speaker":nick,"outcome":{"status":"completed","content":format!("TURN_{round}_{nick}"),"model":"vendor/model","finishReason":"stop","usage":null}}))).collect::<Vec<_>>();
    serde_json::from_value(json!({"topic":{"messageId":ID,"text":"UNIQUE_TOPIC"},"solutions":setup(3).solutions,"transcript":transcript,"instruction":"UNIQUE_INSTRUCTION","rubric":{"version":"v1","criteria":[],"verdictSchema":{}},"judge":{"model":"vendor/judge","promptPackVersion":"1.0.0","renderedTemplateHashes":{}},"roundScoring":{"roundScores":[],"roundTally":{"alpha":0,"beta":0},"unscoredRounds":[1,2,3]}})).unwrap()
}
#[test]
fn opening_requests_both_debaters_have_metadata_only_and_complete_config() {
    let pack = PromptPack::embedded().unwrap();
    let config = config();
    let opening = opening(3);
    for nick in ["alpha", "beta"] {
        let (id, inputs) = pack
            .debate_inputs(
                &config,
                &work("debate.turn", nick, 0),
                &binding(),
                DebateSource::Opening(&opening),
                &work_values(nick, 3),
            )
            .unwrap();
        let request = pack.render_composition(id, &inputs, false).unwrap();
        assert_eq!(id, "debate.round0");
        assert_eq!(inputs["turn_word_limit"], 321);
        assert_eq!(inputs["round_data_json"].as_object().unwrap().len(), 6);
        assert!(!request.user.contains("SOLUTION"));
        assert!(!request.user.contains("UNIQUE_TOPIC"));
        assert!(request.system.contains("alpha") && request.system.contains("beta"));
        assert!(request.system.contains("Jeremy"));
        assert_eq!(
            inputs["persona_json"],
            if nick == "alpha" {
                json!("Be precise {{rogue}}")
            } else {
                json!("")
            }
        );
        let source = pack
            .render_debate_source(DebateSource::Opening(&opening))
            .unwrap();
        assert_eq!(source.matches("UNIQUE_TOPIC").count(), 1);
    }
}
#[test]
fn complete_final_composition_uses_final_limit_for_one_and_many_rounds() {
    let pack = PromptPack::embedded().unwrap();
    let config = config();
    for rounds in [1, 3] {
        for round in 1..=rounds {
            let (id, inputs) = pack
                .debate_inputs(
                    &config,
                    &work("debate.turn", "alpha", round),
                    &binding(),
                    DebateSource::Setup(&setup(rounds)),
                    &work_values("alpha", rounds),
                )
                .unwrap();
            let request = pack.render_composition(id, &inputs, false).unwrap();
            assert_eq!(
                inputs["turn_word_limit"],
                if round == rounds { 543 } else { 321 }
            );
            assert_eq!(id.starts_with("debate.final."), round == rounds);
            if round == rounds {
                assert!(!request.user.contains("321"));
                assert!(request.user.matches("543").count() >= 2);
            }
            let source = pack
                .render_debate_source(DebateSource::Setup(&setup(rounds)))
                .unwrap();
            assert_eq!(source.matches("UNIQUE_TOPIC").count(), 1);
            assert_eq!(source.matches("SOLUTION_A").count(), 1);
        }
    }
}
#[test]
fn round_judge_is_case_bound_and_excludes_later_turns_and_all_scores() {
    let pack = PromptPack::embedded().unwrap();
    let config = config();
    let bundle = bundle();
    let work = work("judge.round", "judge", 2);
    let (id, inputs) = pack
        .judge_inputs(
            &config,
            &work,
            &binding(),
            ID.parse().unwrap(),
            CaseInput::Debate(&bundle),
            None,
        )
        .unwrap();
    assert!(!inputs.contains_key("persona_json"));
    let request = pack.render_composition(id, &inputs, false).unwrap();
    assert!(!request.system.contains("Be precise"));
    assert_eq!(request.user.matches("TURN_2_alpha").count(), 1);
    assert!(request.user.contains("TURN_1_beta"));
    assert!(!request.user.contains("TURN_3"));
    assert!(
        !inputs["round_judge_data_json"]
            .as_object()
            .unwrap()
            .contains_key("roundScoring")
    );
    assert!(
        pack.judge_inputs(
            &config,
            &work,
            &binding(),
            OTHER.parse().unwrap(),
            CaseInput::Debate(&bundle),
            None
        )
        .is_err()
    );
    let schema = &inputs["round_score_schema_json"];
    assert_eq!(
        schema["properties"]["scores"]["required"],
        json!(["alpha", "beta"])
    );
    assert_eq!(
        schema["$defs"]["RoundScore"]["properties"]["reason"]["maxLength"],
        200
    );
}
#[test]
fn final_judge_includes_application_tally_unscored_rounds_and_repair_once() {
    let pack = PromptPack::embedded().unwrap();
    let config = config();
    let bundle = bundle();
    let mut work = work("judge", "judge", 3);
    work.attempt = 2;
    let repair = JudgeRepairData {
        attempt: 2,
        rejected_output: "REJECTED {{rogue}}".into(),
        validation_errors: vec!["BAD_SCHEMA".into()],
    };
    let (id, inputs) = pack
        .judge_inputs(
            &config,
            &work,
            &binding(),
            ID.parse().unwrap(),
            CaseInput::Debate(&bundle),
            Some(&repair),
        )
        .unwrap();
    let request = pack.render_composition(id, &inputs, true).unwrap();
    assert_eq!(request.user.matches("UNIQUE_TOPIC").count(), 1);
    assert_eq!(request.user.matches("REJECTED").count(), 1);
    assert_eq!(
        inputs["judge_data_json"]["roundScoring"]["unscoredRounds"],
        json!([1, 2, 3])
    );
    assert!(
        inputs["verdict_schema_json"]["properties"]
            .get("roundTally")
            .is_none()
    );
}
#[test]
fn standalone_quotes_and_followup_use_only_supplied_case_material() {
    let pack = PromptPack::embedded().unwrap();
    let config = config();
    let mut work = work("judge", "judge", 1);
    work.debate_id = None;
    work.contest_id = None;
    work.round = None;
    let quotes = vec![ResolvedQuote {
        event_id: ID.parse().unwrap(),
        author: Actor::Member(MemberNick::new("alpha").unwrap()),
        text: "QUOTED_EVIDENCE".into(),
    }];
    let (id, inputs) = pack
        .judge_inputs(
            &config,
            &work,
            &binding(),
            ID.parse().unwrap(),
            CaseInput::Standalone {
                question: "QUESTION",
                quotes: &quotes,
            },
            None,
        )
        .unwrap();
    let request = pack.render_composition(id, &inputs, false).unwrap();
    assert_eq!(request.user.matches("QUOTED_EVIDENCE").count(), 1);
    assert!(request.user.contains("alpha"));
    assert!(!request.user.contains("roundTally"));
    let (id, inputs) = pack
        .judge_inputs(
            &config,
            &work,
            &binding(),
            ID.parse().unwrap(),
            CaseInput::Followup {
                question: "FOLLOWUP",
                history: &[],
            },
            None,
        )
        .unwrap();
    let request = pack.render_composition(id, &inputs, false).unwrap();
    assert!(!request.user.contains("QUOTED_EVIDENCE"));
    assert!(
        pack.judge_inputs(
            &config,
            &work,
            &binding(),
            OTHER.parse().unwrap(),
            CaseInput::Followup {
                question: "FOLLOWUP",
                history: &[]
            },
            None
        )
        .is_err()
    );
}
#[test]
fn duplicate_source_projections_suppress_repeated_setup_without_collapsing_other_events() {
    let entry = |id: &str| {
        serde_json::from_value::<HistoryEntry>(json!({"position":"1","role":"user","name":"squad","content":"same text","meta":{"originEventId":id,"originSeq":"1","author":"system","projectionKey":"setup"}})).unwrap()
    };
    let entries = vec![entry(ID), entry(ID), entry(OTHER)];
    assert_eq!(unique_history(&entries).len(), 2);
}
#[test]
fn renderer_trait_keeps_json_native_and_returns_one_composition() {
    let pack = PromptPack::embedded().unwrap();
    let config = config();
    let (id, inputs) = pack
        .debate_inputs(
            &config,
            &work("debate.turn", "alpha", 0),
            &binding(),
            DebateSource::Opening(&opening(1)),
            &work_values("alpha", 1),
        )
        .unwrap();
    let values = inputs
        .into_iter()
        .map(|(key, value)| {
            let serialized = if key.ends_with("_json") || !value.is_string() {
                serde_json::to_string(&value).unwrap()
            } else {
                value.as_str().unwrap().to_owned()
            };
            (key, serialized)
        })
        .collect();
    let prompt = PromptRef {
        template_id: Text::new(id).unwrap(),
        pack_version: Text::new(pack.version()).unwrap(),
        template_hash: Text::new("frozen").unwrap(),
    };
    let rendered: Value =
        serde_json::from_str(&PromptRenderer::render(&pack, &prompt, &values).unwrap()).unwrap();
    assert_eq!(rendered["provider_user_name"], "squad");
    assert!(rendered["system"].as_str().unwrap().contains("{{rogue}}"));
}

#[test]
fn accepted_verdict_announcement_contains_tally_and_round_records() {
    let pack = PromptPack::embedded().unwrap();
    let debaters = [
        MemberNick::new("alpha").unwrap(),
        MemberNick::new("beta").unwrap(),
    ];
    let tally = debaters
        .iter()
        .cloned()
        .map(|n| (n, 0))
        .collect::<BTreeMap<_, _>>();
    let judge = json!({"winner":"alpha","scores":(["alpha","beta"].map(|nick|json!({"nick":nick,"criteria":(["fit","feasibility","originality","survivedAttacks"].map(|c|json!({"criterionId":c,"score":5,"reasoning":"reason"})))}))),"reasoning":"Equal totals resolved by fit","whatToStealFromLoser":[],"tallyOverrideReason":null});
    let verdict = verdict::validate_judge_verdict(&judge, &debaters, &tally).unwrap();
    let state:DebateState=serde_json::from_value(json!({"debateId":ID,"state":"COMPLETED","debaters":debaters,"round":1,"totalRounds":1,"judgeModel":"vendor/judge","judgeRequested":true,"stateVersion":1,"terminalReason":null,"caseId":ID,"caseIds":[ID]})).unwrap();
    let mut value = json!({"messageId":ID,"workId":ID,"triggerEventId":ID,"attempt":1,"model":"vendor/judge","finishReason":"stop","usage":null,"debateId":ID,"caseId":ID,"judgeInvocationId":ID,"rubricVersion":"v1"});
    value.as_object_mut().unwrap().extend(
        serde_json::to_value(verdict)
            .unwrap()
            .as_object()
            .unwrap()
            .clone(),
    );
    let accepted: DebateVerdict = serde_json::from_value(value).unwrap();
    let score:DebateRoundScore=serde_json::from_value(json!({"debateId":ID,"caseId":ID,"round":1,"scores":null,"tally":tally,"reason":"unscored"})).unwrap();
    let notice = pack
        .render_verdict_announcement(&accepted, &state, &tally, &[score])
        .unwrap();
    assert!(notice.starts_with("[squad notice]"));
    assert!(notice.contains("alpha wins"));
    assert!(notice.contains("roundTally"));
    assert!(notice.contains("unscored"));
    let mut invalid = accepted;
    invalid.verdict.verdict.winner = MemberNick::new("outsider").unwrap();
    assert!(
        pack.render_verdict_announcement(&invalid, &state, &tally, &[])
            .is_err()
    );
}
#[test]
fn shared_moves_select_compositions_without_hidden_model_calls() {
    for (round, total, id) in [
        (0, 3, "debate.round0"),
        (1, 1, "debate.final.round1"),
        (2, 3, "debate.rebuttal"),
        (3, 3, "debate.final.rebuttal"),
    ] {
        assert_eq!(
            shared_move_composition(
                &ContestMove::Turns,
                &work("debate.turn", "alpha", round),
                total
            )
            .unwrap(),
            Some(id)
        );
    }
    for move_name in [
        ContestMove::Pick,
        ContestMove::Tally,
        ContestMove::Barrier,
        ContestMove::Report,
    ] {
        assert_eq!(
            shared_move_composition(&move_name, &work("judge", "judge", 1), 1).unwrap(),
            None
        );
    }
    assert!(shared_move_composition(&ContestMove::Collect, &work("judge", "judge", 1), 1).is_err());
}

#[test]
fn frozen_debate_metadata_rejects_wrong_opponent_and_round_count() {
    let pack = PromptPack::embedded().unwrap();
    let config = config();
    let work = work("debate.turn", "alpha", 1);
    let setup = setup(3);
    for values in [
        BTreeMap::new(),
        BTreeMap::from([
            ("opponent".into(), "alpha".into()),
            ("rounds".into(), "3".into()),
        ]),
        BTreeMap::from([
            ("opponent".into(), "beta".into()),
            ("rounds".into(), "2".into()),
        ]),
    ] {
        assert!(
            pack.debate_inputs(
                &config,
                &work,
                &binding(),
                DebateSource::Setup(&setup),
                &values
            )
            .is_err()
        );
    }
}
#[test]
fn final_evidence_preserves_round_records_reasons_and_router_tally() {
    let pack = PromptPack::embedded().unwrap();
    let config = config();
    let mut bundle = bundle();
    bundle.round_scoring.round_scores=serde_json::from_value(json!([
        {"debateId":ID,"caseId":ID,"round":2,"scores":null,"tally":{"alpha":1,"beta":0},"reason":"unscored"},
        {"debateId":ID,"caseId":ID,"round":1,"scores":{"alpha":{"score":8,"reason":"UNIQUE_REASON"},"beta":{"score":7,"reason":"other"}},"tally":{"alpha":1,"beta":0}}
    ])).unwrap();
    bundle
        .round_scoring
        .round_tally
        .insert(MemberNick::new("alpha").unwrap(), 1);
    let (id, inputs) = pack
        .judge_inputs(
            &config,
            &work("judge", "judge", 3),
            &binding(),
            ID.parse().unwrap(),
            CaseInput::Debate(&bundle),
            None,
        )
        .unwrap();
    let scoring = &inputs["judge_data_json"]["roundScoring"];
    assert_eq!(scoring["roundScores"][0]["round"], 1);
    assert_eq!(scoring["roundScores"][1]["scores"], Value::Null);
    assert_eq!(scoring["roundTally"]["alpha"], 1);
    let rendered = pack.render_composition(id, &inputs, false).unwrap();
    assert_eq!(rendered.user.matches("UNIQUE_REASON").count(), 1);
    let (_, member) = pack
        .debate_inputs(
            &config,
            &work("debate.turn", "alpha", 3),
            &binding(),
            DebateSource::Setup(&setup(3)),
            &work_values("alpha", 3),
        )
        .unwrap();
    assert!(
        !serde_json::to_string(&member)
            .unwrap()
            .contains("UNIQUE_REASON")
    );
}

#[test]
fn grill_compositions_use_frozen_roles_rounds_angles_and_limits() {
    let pack = PromptPack::embedded().unwrap();
    let config = config();
    let frozen = GrillPreflight {
        setup: GrillSetup {
            contest_id: ID.parse().unwrap(),
            leader: "alpha".parse().unwrap(),
            grillers: vec!["beta".parse().unwrap()],
            source_message_id: ID.parse().unwrap(),
            initial_plan: "Plan".into(),
            focus: "Focus".into(),
            total_rounds: 2,
        },
        answer_words: 321,
        plan_words: 1200,
    };
    let mut state = squad_core::orchestrator::grill::initial_state(&frozen.setup).unwrap();
    let mut w = work("grill.question", "beta", 1);
    w.debate_id = None;
    w.step_id = Some(ID.parse().unwrap());
    state.state = GrillStatus::Questioning;
    for (kind, status, actor, id) in [
        (
            WorkKind::GrillQuestion,
            GrillStatus::Questioning,
            "beta",
            "grill.question",
        ),
        (
            WorkKind::GrillAnswer,
            GrillStatus::Answering,
            "alpha",
            "grill.answer",
        ),
        (
            WorkKind::GrillRebuild,
            GrillStatus::Rebuilding,
            "alpha",
            "grill.rebuild",
        ),
    ] {
        w.kind = kind;
        w.prompt_ref.as_mut().unwrap().template_id = Text::new(id).unwrap();
        w.target = Target::Member(actor.parse().unwrap());
        w.context_id = ContextId(format!("contest:{ID}:{ID}:{actor}"));
        state.state = status;
        let (composition, inputs) = pack
            .grill_inputs(&config, &w, &binding(), &frozen, &state)
            .unwrap();
        assert_eq!(composition, id);
        let rendered = pack.render_composition(id, &inputs, false).unwrap();
        assert!(rendered.user.contains("round 1 of 2"));
        assert!(rendered.user.contains("alpha"));
        assert!(!rendered.user.contains("judge.round"));
        assert_eq!(rendered.system.matches("You are @").count(), 1);
    }
    w.target = Target::Member("beta".parse().unwrap());
    assert!(
        pack.grill_inputs(&config, &w, &binding(), &frozen, &state)
            .is_err()
    );
}
