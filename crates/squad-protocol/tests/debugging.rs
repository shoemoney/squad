use serde_json::json;
use squad_protocol::{debugging::*, *};
fn proposal() -> serde_json::Value {
    json!({"tests":[{"localId":"T1","test":"Resolver lookup","procedure":"Read resolver output","effort":"minutes","safety":"read_only","outcomes":["Failure","Success"],"predictions":[{"hypothesisId":"H1","outcome":"Failure","cannotPredictReason":null},{"hypothesisId":"H2","outcome":"Success","cannotPredictReason":null}]}],"newHypothesis":null})
}
#[test]
fn required_nullable_exclusive_predictions_and_unknown_fields() {
    let p = proposal();
    validate_artifact("debug.test", &p).unwrap();
    for row in [
        json!({"hypothesisId":"H1","outcome":null,"cannotPredictReason":null}),
        json!({"hypothesisId":"H1","outcome":"Failure","cannotPredictReason":"unsure"}),
        json!({"hypothesisId":"H1","outcome":"Failure"}),
        json!({"hypothesisId":"H1","outcome":"Unlisted","cannotPredictReason":null}),
    ] {
        let mut v = p.clone();
        v["tests"][0]["predictions"][0] = row;
        assert!(validate_artifact("debug.test", &v).is_err());
    }
    let mut v = p;
    v["author"] = json!("member");
    assert!(validate_artifact("debug.test", &v).is_err());
}
#[test]
fn bounded_arrays_and_unique_ids() {
    assert!(serde_json::from_value::<DebuggingHypothesisStatus>(json!("confirmed")).is_err());
    let mut p = proposal();
    p["tests"][0]["outcomes"] = json!(["Failure", "Failure"]);
    assert!(validate_artifact("debug.test", &p).is_err());
    let mut p = proposal();
    p["tests"] = json!([p["tests"][0], p["tests"][0]]);
    assert!(validate_artifact("debug.test", &p).is_err());
    assert!(validate_artifact("debug.hypothesize", &json!({"hypotheses":[]})).is_err());
}
#[test]
fn all_work_schemas_and_explicit_debug_transition() {
    for kind in [
        WorkKind::DebuggingHypothesize,
        WorkKind::DebuggingMerge,
        WorkKind::DebuggingTest,
        WorkKind::DebuggingSelect,
        WorkKind::DebuggingReport,
    ] {
        assert!(output_schema(&kind).is_some());
        assert!(composition(&kind).unwrap().starts_with("debug."));
    }
    assert_eq!(
        serde_json::to_value(DebuggingTransition::Advance).unwrap(),
        json!("debug")
    );
    assert!(output_schema(&WorkKind::Reply).is_none());
}
#[test]
fn enabled_parser_example() {
    let ctx = parser::ParseContext {
        roster: ["one", "two", "three"]
            .iter()
            .map(|n| n.parse().unwrap())
            .collect(),
        ..Default::default()
    };
    let ParsedInput::Debugging { limits, rounds, .. } = parse(
        "@all $debug Requests fail\n--\nread-only tests only",
        &Default::default(),
        &ctx,
    )
    .unwrap() else {
        panic!("expected debug")
    };
    assert_eq!(limits.as_deref(), Some("read-only tests only"));
    assert_eq!(rounds, 3);
}
#[test]
fn selection_refs_safety_split_and_patch_permission() {
    let p: DebuggingTestProposal = serde_json::from_value(proposal()).unwrap();
    let mut tests = std::collections::BTreeMap::from([("P:T1".into(), p.tests[0].clone())]);
    let live = vec!["H1".to_string(), "H2".to_string()];
    let valid:DebuggingSelection=serde_json::from_value(json!({"testRef":"P:T1","instruction":"Read resolver output","outcomes":[{"label":"O1","description":"Failure"},{"label":"O2","description":"Success"}],"predictions":[{"hypothesisId":"H1","outcome":"O1"},{"hypothesisId":"H2","outcome":"O2"}],"addedHypotheses":[],"notSelected":[]})).unwrap();
    valid.validate_against(&tests, &live).unwrap();
    let mut invalid = valid.clone();
    invalid.test_ref = Some("invented".into());
    assert!(invalid.validate_against(&tests, &live).is_err());
    invalid = valid.clone();
    invalid.predictions[1].outcome = "O1".into();
    assert!(invalid.validate_against(&tests, &live).is_err());
    tests.get_mut("P:T1").unwrap().safety = DebuggingSafety::Risky;
    assert!(valid.validate_against(&tests, &live).is_err());
    let report = DebuggingSummary {
        summary: "A cause".into(),
        next_test: None,
        patch: Some("Change configuration".into()),
        patch_rationale: "Addresses DNS".into(),
        risks: vec![],
    };
    assert!(report.validate_patch(false).is_err());
    report.validate_patch(true).unwrap();
}
