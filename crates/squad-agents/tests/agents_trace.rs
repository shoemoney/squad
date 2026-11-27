use serde_json::json;
use squad_protocol::{trace::*, *};
#[test]
fn member_contract_drops_no_evidence_and_forbids_identity_metadata() {
    let output = json!({"claims":[{"localId":"L1","claim":"Records retained","quotes":[{"sourceId":"S1","quote":"Records are retained for thirty days."}],"counterQuotes":[],"scopeNote":"Thirty days"}]});
    validate_artifact("trace.extract", &output).unwrap();
    assert!(prose_words(&output) > 5);
    let mut bad = output;
    bad["model"] = json!("provider/model");
    assert!(validate_artifact("trace.extract", &bad).is_err());
    assert_eq!(squad_agents::agents::trace::enabled(), ENABLED);
    assert_eq!(composition(&WorkKind::TraceVerify), Some("trace.verify"));
}
#[test]
fn verification_contract_requires_complete_typed_verdicts() {
    let good = json!({"verdicts":[{"claimId":"C1","entailment":"partial","scope":"missing_qualifier","qualifierQuote":{"sourceId":"S1","quote":"Only if the operator enables retention."},"note":"Conditional support"}]});
    validate_artifact("trace.verify", &good).unwrap();
    let mut bad = good;
    bad["verdicts"][0]["scope"] = json!("confirmed");
    assert!(validate_artifact("trace.verify", &bad).is_err());
}
