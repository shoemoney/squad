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
    assert!(matches!(
        parse(
            "@all $debug Requests fail -- read-only tests only",
            &Default::default(),
            &ctx
        )
        .unwrap(),
        ParsedInput::Debugging { .. }
    ));
}
