use serde_json::json;
use squad_protocol::{readback::*, *};
fn entry() -> serde_json::Value {
    json!({"entryId":"one","passage":"The service retains records.","question":"For how long?","readings":[{"readingId":"a","text":"Thirty days","sourceLabels":["Reader 1"]},{"readingId":"b","text":"Forever","sourceLabels":["Reader 2"]}],"behaviourChange":"Records expire or persist"})
}
#[test]
fn ledger_requires_real_different_sourced_readings_and_unique_ids() {
    let value = json!({"entries":[entry()]});
    validate_artifact("readback.diff", &value).unwrap();
    for change in 0..5 {
        let mut invalid = value.clone();
        match change {
            0 => invalid["entries"][0]["readings"] = json!([entry()["readings"][0].clone()]),
            1 => invalid["entries"][0]["readings"][0]["sourceLabels"] = json!([]),
            2 => {
                invalid["entries"][0]["readings"][1] = invalid["entries"][0]["readings"][0].clone()
            }
            3 => invalid["entries"][0]["surprise"] = json!(true),
            _ => invalid["entries"] = json!([entry(), entry()]),
        }
        assert!(
            validate_artifact("readback.diff", &invalid).is_err(),
            "{invalid}"
        );
    }
}
#[test]
fn strict_member_rewrite_and_provenance_schemas() {
    let output =
        json!({"build":"Implement retention","behaviours":[],"edgeCases":[],"assumptions":[]});
    validate_artifact("readback.interpret", &output).unwrap();
    let mut wrong = output;
    wrong["member"] = json!("member0");
    assert!(validate_artifact("readback.interpret", &wrong).is_err());
    let mut rw = json!({"spec":"New spec","changes":[{"entryId":"one","before":"Old passage here","after":"New passage here"}]});
    validate_artifact("readback.rewrite", &rw).unwrap();
    let duplicate = rw["changes"][0].clone();
    rw["changes"].as_array_mut().unwrap().push(duplicate);
    assert!(validate_artifact("readback.rewrite", &rw).is_err());
    for kind in [
        WorkKind::ReadbackInterpret,
        WorkKind::ReadbackDiff,
        WorkKind::ReadbackRewrite,
    ] {
        assert!(output_schema(&kind).unwrap().is_object());
    }
    assert!(output_schema(&WorkKind::Reply).is_none());
}
#[test]
fn enabled_parser_and_choice_grammar() {
    assert!(ENABLED);
    let mut ctx = parser::ParseContext {
        roster: ["a", "b", "c"].map(|s| s.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    assert!(matches!(
        parse(
            "@all $readback The service retains records.",
            &Default::default(),
            &ctx
        )
        .unwrap(),
        ParsedInput::Readback { rounds: 2, .. }
    ));
    ctx.pending_human = Some(parser::PendingHuman {
        contest_id: ContestId::new(),
        kind: ContestKind::Readback,
        state_version: 2,
        step_id: StepId::new(),
        prompt: "Pick".into(),
        items: vec!["One".into(), "Two".into()],
        choices: vec![vec!["A".into(), "B".into()], vec!["A".into(), "B".into()]],
        outcome_labels: vec![],
        answers: Default::default(),
    });
    for good in ["1b 2a", "/answer 1: My own meaning", "@a ordinary message"] {
        assert!(parse(good, &Default::default(), &ctx).is_ok(), "{good}");
    }
    for bad in ["1c", "3a", "/answer 4: absent"] {
        assert!(parse(bad, &Default::default(), &ctx).is_err(), "{bad}");
    }
}
