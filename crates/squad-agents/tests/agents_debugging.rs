use serde_json::json;
use squad_protocol::{debugging::*, *};
#[test]
fn executor_contract_and_prose_budget_exclude_identity() {
    let output = json!({"hypotheses":[{"localId":"one","cause":"Resolver failure","supporting":[],"contradicting":[],"prior":"high"}]});
    validate_artifact("debug.hypothesize", &output).unwrap();
    assert_eq!(prose_words(&output), 2);
    let mut invalid = output;
    invalid["model"] = json!("provider/model");
    assert!(validate_artifact("debug.hypothesize", &invalid).is_err());
    assert_eq!(squad_agents::agents::debugging::enabled(), ENABLED);
    assert_eq!(composition(&WorkKind::DebuggingTest), Some("debug.test"));
}
