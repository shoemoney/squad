use serde_json::json;
use squad_protocol::{decide::*, *};
#[test]
fn criteria_weights_cells_and_strict_judge_shape() {
    let good = json!({"criteria":[{"localId":"a","name":"Cost","definition":"Cheaper is better","weight":100}]});
    validate_artifact("decide.criteria", &good).unwrap();
    let mut bad = good;
    bad["criteria"][0]["weight"] = json!(99);
    assert!(validate_artifact("decide.criteria", &bad).is_err());
    let data =
        json!({"options":[{"optionId":"a"},{"optionId":"b"}],"criteria":[{"criterionId":"c"}]});
    let cell = |o: &str, score: u8| json!({"optionId":o,"criterionId":"c","score":score,"evidence":"Supported assumption"});
    let good = json!({"cells":[cell("a",5),cell("b",7)]});
    validate_output(&WorkKind::DecideScore, &data, &good).unwrap();
    for cells in [
        vec![cell("a", 5)],
        vec![cell("a", 5), cell("a", 7)],
        vec![cell("a", 11), cell("b", 7)],
        vec![cell("a", 5), cell("unknown", 7)],
    ] {
        assert!(validate_output(&WorkKind::DecideScore, &data, &json!({"cells":cells})).is_err());
    }
    assert!(validate_artifact("decide.freeze",&json!({"criteria":[{"criterionId":"c","name":"Cost","definition":"Cheaper","sources":[]}]})).is_err());
    assert!(
        validate_artifact(
            "decide.explain",
            &json!({"winner":"a","flipConditions":[],"dissents":[],"evidenceGaps":[]})
        )
        .is_err()
    );
}
#[test]
fn merges_preserve_every_source_once() {
    let input = json!({"proposals":[{"memberLabel":"M1","criteria":[{"localId":"a"}]},{"memberLabel":"M2","criteria":[{"localId":"b"}]}]});
    let good = json!({"criteria":[{"criterionId":"c","name":"Cost","definition":"Lower is better","sources":[{"memberLabel":"M1","localId":"a"},{"memberLabel":"M2","localId":"b"}]}]});
    validate_output(&WorkKind::DecideFreeze, &input, &good).unwrap();
    let mut bad = good.clone();
    bad["criteria"][0]["sources"][1]["localId"] = json!("invented");
    assert!(validate_output(&WorkKind::DecideFreeze, &input, &bad).is_err());
    let mut bad = good;
    bad["criteria"][0]["sources"].as_array_mut().unwrap().pop();
    assert!(validate_output(&WorkKind::DecideFreeze, &input, &bad).is_err());
}
#[test]
fn parser_switch_gates_decide_and_refuses_bad_option_counts() {
    let ctx = parser::ParseContext {
        roster: ["a", "b", "c"].map(|s| s.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    let meta = parser::SubmissionMeta::default();
    let parsed = parse("@all $decide Choose\n--\nOne\nTwo", &meta, &ctx);
    if ENABLED {
        assert!(parsed.is_ok(), "{parsed:?}");
    } else {
        assert!(parsed.is_err());
    }
    for opts in ["One", "One\nTwo\nThree\nFour\nFive"] {
        assert!(parse(&format!("@all $decide Choose\n--\n{opts}"), &meta, &ctx).is_err());
    }
}
