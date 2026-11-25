use serde_json::json;
use squad_agents::agents::census::validate;
use squad_protocol::*;
#[test]
fn census_fixtures_enforce_anonymous_rating_coverage_and_exact_citations() {
    let data = json!({"plan":"We back up daily.","planVersion":1,"ledger":[{"id":"g1","omission":"Backups","consequence":"Data loss"}]});
    let good = json!({"ratings":[{"gapId":"g1","rating":"covered","reason":"Already handled","citation":{"location":"plan","quote":"back up daily"}}],"additions":[]});
    validate(&WorkKind::CensusRate, &data, &good).unwrap();
    let mut fabricated = good.clone();
    fabricated["ratings"][0]["citation"]["quote"] = json!("imaginary text");
    assert!(validate(&WorkKind::CensusRate, &data, &fabricated).is_err());
    let missing = json!({"ratings":[],"additions":[]});
    assert!(validate(&WorkKind::CensusRate, &data, &missing).is_err());
    let mut attributed = good;
    attributed["finderCount"] = json!(6);
    assert!(validate(&WorkKind::CensusRate, &data, &attributed).is_err());
}
#[test]
fn merge_sources_are_exact_and_models_cannot_supply_estimates() {
    let data = json!({"sourceIds":["find1","find2"]});
    let merge = json!({"ledger":[{"id":"g1","omission":"Backups","consequence":"Data loss","lenses":["failure modes"],"sourceFindIds":["find1","find2"]}]});
    validate(&WorkKind::CensusMerge, &data, &merge).unwrap();
    let mut duplicate = merge;
    duplicate["ledger"][0]["sourceFindIds"] = json!(["find1", "find1"]);
    assert!(validate(&WorkKind::CensusMerge, &data, &duplicate).is_err());
    let data = json!({"outcomes":[],"additionIds":[]});
    let valid = json!({"confirmed":[],"covered":[],"trivial":[],"additions":[]});
    validate(&WorkKind::CensusFinal, &data, &valid).unwrap();
    let mut invented = valid;
    invented["unseen"] = json!(0);
    invented["riskAcceptance"] = json!(true);
    assert!(validate(&WorkKind::CensusFinal, &data, &invented).is_err());
}
