use serde_json::json;
use squad_protocol::{forge::*, *};
#[test]
fn strict_outputs_and_reference_validation() {
    let input = json!({"labels":["D1","D2"],"wordLimit":500});
    let valid = json!({"ranking":["D2","D1"],"notes":[{"label":"D1","steal":"Opening","fatalFlaw":null},{"label":"D2","steal":"Ending","fatalFlaw":"Too vague"}]});
    assert!(validate_output(&WorkKind::ForgeRank, &input, &valid).is_ok());
    for change in [json!(["D1", "D1"]), json!(["D1"]), json!(["D1", "own"])] {
        let mut bad = valid.clone();
        bad["ranking"] = change;
        assert!(validate_output(&WorkKind::ForgeRank, &input, &bad).is_err());
    }
    let mut missing = valid.clone();
    missing["notes"][0]
        .as_object_mut()
        .unwrap()
        .remove("fatalFlaw");
    assert!(validate_output(&WorkKind::ForgeRank, &input, &missing).is_err());
    let mut unknown = valid;
    unknown["score"] = json!(100);
    assert!(validate_output(&WorkKind::ForgeRank, &input, &unknown).is_err());
}
#[test]
fn fusion_length_provenance_and_judge_cannot_edit() {
    let input = json!({"labels":["D1","D2"],"wordLimit":1200,"maxWords":2});
    let mut fusion = json!({"body":"two words","provenance":[{"element":"opening","sourceLabels":["D1"]}],"handledFlaws":[]});
    assert!(validate_output(&WorkKind::ForgeFuse, &input, &fusion).is_ok());
    fusion["body"] = json!("three full words");
    assert!(validate_output(&WorkKind::ForgeFuse, &input, &fusion).is_err());
    fusion["body"] = json!("two words");
    fusion["provenance"][0]["sourceLabels"] = json!(["unknown"]);
    assert!(validate_output(&WorkKind::ForgeFuse, &input, &fusion).is_err());
    let mut verdict = json!({"ranking":["D1","D2"],"reasons":[{"label":"D1","reason":"clear"},{"label":"D2","reason":"vague"}],"seamIssues":[]});
    assert!(validate_output(&WorkKind::ForgeJudge, &input, &verdict).is_ok());
    verdict["body"] = json!("Edited winner");
    assert!(validate_output(&WorkKind::ForgeJudge, &input, &verdict).is_err());
}
#[test]
fn parser_switch_and_typed_schema() {
    assert_eq!(ContestKind::Forge.is_built(), ENABLED);
    let ctx = parser::ParseContext {
        roster: ["a", "b", "c"].map(|n| n.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    let parsed = parse(
        "@all $forge Write a launch post -- Under 100 words",
        &parser::SubmissionMeta::default(),
        &ctx,
    );
    if ENABLED {
        assert!(matches!(parsed.unwrap(), ParsedInput::Forge { .. }));
    } else {
        assert!(parsed.is_err());
    }
    for k in [
        WorkKind::ForgeDraft,
        WorkKind::ForgeRank,
        WorkKind::ForgeFuse,
        WorkKind::ForgeJudge,
    ] {
        assert!(output_schema(&k).is_some());
    }
}
