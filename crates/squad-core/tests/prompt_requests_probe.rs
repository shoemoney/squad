use serde_json::{Value, json};
use squad_core::prompts::PromptPack;
use squad_protocol::*;
use std::collections::BTreeMap;
const ID: &str = "01900000-0000-7000-8000-000000000001";
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
fn mode_work(kind: &str, target: &str, attempt: u32) -> Work {
    let judge = target == "judge";
    let context = if judge {
        format!("judge:{ID}")
    } else {
        format!("contest:{ID}:{ID}:{target}")
    };
    let work: Work = serde_json::from_value(json!({"workId":ID,"messageId":ID,"kind":kind,"target":target,"contextId":context,"triggerEventId":ID,"historyThrough":"10","attempt":attempt,"contestId":ID,"stepId":ID,"caseId":if judge {json!(ID)} else {Value::Null},"maxOutputTokens":if judge {4096} else {2048},"promptRef":{"templateId":kind,"packVersion":"1.0.0","templateHash":"frozen"}})).unwrap();
    work.validate().unwrap_or_else(|e| panic!("{kind}: {e:?}"));
    work
}
fn t(s: &str) -> Text {
    Text::new(s).unwrap()
}
fn all_text(r: &squad_core::prompts::RenderedComposition) -> String {
    format!("{}\n{}", r.system, r.user)
}

fn brief() -> ProbeBrief {
    ProbeBrief {
        observation: t("OBSERVATION_CANARY"),
        ruled_out: "RULED_OUT_CANARY".into(),
        inspectable: t("INSPECTABLE_CANARY"),
    }
}
fn explanation(text: &str) -> ProbeItem<ProbeExplanation> {
    ProbeItem {
        id: CellId::new(),
        data: ProbeExplanation {
            explanation: t(text),
            key_assumption: t("KEY_ASSUMPTION_TEXT"),
            limitations: vec![],
        },
    }
}
fn check(text: &str) -> ProbeItem<ProbeCheck> {
    ProbeItem {
        id: CellId::new(),
        data: ProbeCheck {
            check: t(text),
            procedure: t("procedure"),
            measurement: t("measurement"),
            outcomes: vec![
                ProbeOutcome {
                    id: t("o1"),
                    description: t("one"),
                },
                ProbeOutcome {
                    id: t("o2"),
                    description: t("two"),
                },
            ],
            effort: t("low"),
            safety_notes: vec![t("safe")],
        },
    }
}
fn render(
    work: &Work,
    input: &ProbeInput,
    repair: Option<&JudgeRepairData>,
) -> squad_core::prompts::RenderedComposition {
    let pack = PromptPack::embedded().unwrap();
    let (id, inputs) = pack
        .probe_inputs(&config(), work, &binding(), input, repair)
        .unwrap();
    assert_eq!(Some(id), work.kind.probe_composition());
    pack.render_composition(id, &inputs, repair.is_some())
        .unwrap()
}

#[test]
fn initial_requests_are_blind_and_identical_for_explainers_and_checkers() {
    let input = ProbeInput::Propose { brief: brief() };
    for kind in ["probe.explain", "probe.check"] {
        let r = render(&mode_work(kind, "alpha", 1), &input, None);
        for canary in [
            "OBSERVATION_CANARY",
            "RULED_OUT_CANARY",
            "INSPECTABLE_CANARY",
        ] {
            assert_eq!(r.user.matches(canary).count(), 1, "{kind} {canary}");
        }
        let text = all_text(&r);
        assert!(text.contains("participant"), "blind identity");
        // Blind contexts: no real nick, model slug or persona anywhere.
        for forbidden in [
            "vendor/alpha",
            "vendor/beta",
            "Be precise",
            "\"alpha\"",
            "beta",
        ] {
            assert!(!text.contains(forbidden), "{kind} leaked {forbidden}");
        }
        assert_eq!(r.provider_user_name, "squad");
    }
}

#[test]
fn checkers_see_no_explanations_and_explainers_no_rival_explanations_or_predictions() {
    let propose = ProbeInput::Propose { brief: brief() };
    for kind in ["probe.explain", "probe.check"] {
        let text = all_text(&render(&mode_work(kind, "alpha", 1), &propose, None));
        assert!(!text.contains("EXPLANATION_") && !text.contains("PREDICT"));
    }
    // A propose input cannot hold explanations or checks, and a prediction input
    // cannot be handed to the proposing kinds.
    let predict = ProbeInput::Predict {
        brief: brief(),
        explanation: explanation("OWN_EXPLANATION"),
        checks: vec![check("CHECK_ONE"), check("CHECK_TWO")],
    };
    let pack = PromptPack::embedded().unwrap();
    for kind in ["probe.explain", "probe.check"] {
        assert!(
            pack.probe_inputs(
                &config(),
                &mode_work(kind, "alpha", 1),
                &binding(),
                &predict,
                None
            )
            .is_err()
        );
    }
}

#[test]
fn predictions_receive_only_their_own_accepted_explanation_and_every_check() {
    let predict = ProbeInput::Predict {
        brief: brief(),
        explanation: explanation("OWN_EXPLANATION"),
        checks: vec![check("CHECK_ONE"), check("CHECK_TWO")],
    };
    let r = render(&mode_work("probe.predict", "alpha", 1), &predict, None);
    for canary in [
        "OWN_EXPLANATION",
        "CHECK_ONE",
        "CHECK_TWO",
        "OBSERVATION_CANARY",
    ] {
        assert_eq!(r.user.matches(canary).count(), 1, "{canary}");
    }
    let text = all_text(&r);
    assert!(!text.contains("RIVAL_EXPLANATION"));
    // Probe report: judge system, no persona, every record once.
    let material = ProbeCaseMaterial {
        brief: brief(),
        explanations: vec![explanation("EXPL_A"), explanation("EXPL_B")],
        checks: vec![check("CHECK_ONE")],
        prediction_matrix: BTreeMap::new(),
        missing_rows: vec![],
        skipped_members: vec![],
    };
    let report = ProbeInput::Report { material };
    let r = render(&mode_work("probe.report", "judge", 1), &report, None);
    for canary in ["EXPL_A", "EXPL_B", "CHECK_ONE"] {
        assert_eq!(r.user.matches(canary).count(), 1, "{canary}");
    }
    assert!(!r.system.contains("Be precise"));
}

#[test]
fn only_the_report_may_repair_and_only_for_its_own_authorized_attempt() {
    let repair = JudgeRepairData {
        attempt: 2,
        rejected_output: "REJECTED_ONCE".into(),
        validation_errors: vec!["ERROR_ONCE".into()],
    };
    let material = ProbeCaseMaterial {
        brief: brief(),
        explanations: vec![explanation("EXPL_A")],
        checks: vec![check("CHECK_ONE")],
        prediction_matrix: BTreeMap::new(),
        missing_rows: vec![],
        skipped_members: vec![],
    };
    let report = ProbeInput::Report { material };
    let r = render(
        &mode_work("probe.report", "judge", 2),
        &report,
        Some(&repair),
    );
    assert_eq!(r.user.matches("REJECTED_ONCE").count(), 1);
    assert_eq!(r.user.matches("ERROR_ONCE").count(), 1);
    assert_eq!(r.user.matches("EXPL_A").count(), 1);
    let pack = PromptPack::embedded().unwrap();
    // Wrong attempt for the repair record.
    assert!(
        pack.probe_inputs(
            &config(),
            &mode_work("probe.report", "judge", 3),
            &binding(),
            &report,
            Some(&repair)
        )
        .is_err()
    );
    // Member steps have no repair channel.
    let propose = ProbeInput::Propose { brief: brief() };
    assert!(
        pack.probe_inputs(
            &config(),
            &mode_work("probe.check", "alpha", 2),
            &binding(),
            &propose,
            Some(&repair)
        )
        .is_err()
    );
}
