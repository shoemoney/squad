mod common;
use common::*;
use serde_json::json;
use squad_protocol::*;
use std::collections::BTreeMap;
fn nicks() -> [Nick; 2] {
    [Nick::new("a").unwrap(), Nick::new("b").unwrap()]
}
fn tally(a: u32, b: u32) -> BTreeMap<Nick, u32> {
    [(nicks()[0].clone(), a), (nicks()[1].clone(), b)].into()
}
#[test]
fn rubric_totals_winner_coverage_and_override_are_validated() {
    let v = judge();
    let verdict = verdict::validate_judge_verdict(&v, &nicks(), &tally(0, 0)).unwrap();
    assert!(verdict.weighted_totals.iter().all(|v| v.total == 5.0));
    assert!(verdict::validate_judge_verdict(&v, &nicks(), &tally(0, 10)).is_err());
    let mut v = v;
    v["tallyOverrideReason"] = json!("The final proposal improved.");
    assert!(verdict::validate_judge_verdict(&v, &nicks(), &tally(0, 10)).is_ok());
    for bad in [json!(null), json!("tie"), json!("c")] {
        let mut x = v.clone();
        x["winner"] = bad;
        assert!(verdict::validate_judge_verdict(&x, &nicks(), &tally(0, 0)).is_err());
    }
    let mut x = v.clone();
    x["scores"][0]["criteria"][0]["score"] = json!(1.5);
    assert!(verdict::validate_judge_verdict(&x, &nicks(), &tally(0, 0)).is_err());
    let mut x = v.clone();
    x["scores"][0]["criteria"][0]["criterionId"] = json!("feasibility");
    assert!(verdict::validate_judge_verdict(&x, &nicks(), &tally(0, 0)).is_err());
    let mut x = v.clone();
    x["scores"][0]["nick"] = json!("b");
    assert!(verdict::validate_judge_verdict(&x, &nicks(), &tally(0, 0)).is_err());
    let mut x = v.clone();
    x["scores"][1]["criteria"][0]["score"] = json!(10);
    assert!(verdict::validate_judge_verdict(&x, &nicks(), &tally(0, 0)).is_err());
    for (field, limit) in [("reasoning", 12000), ("tallyOverrideReason", 0)] {
        let mut x = v.clone();
        x[field] = json!(if limit == 0 {
            " ".into()
        } else {
            "x".repeat(limit + 1)
        });
        assert!(verdict::validate_judge_verdict(&x, &nicks(), &tally(0, 10)).is_err());
    }
}
#[test]
fn provider_cannot_supply_authoritative_fields_or_omit_nullable() {
    for field in ["roundTally", "weightedTotals", "unexpected"] {
        let mut v = judge();
        v[field] = json!({});
        assert!(verdict::validate_judge_verdict(&v, &nicks(), &tally(0, 0)).is_err());
    }
    let mut v = judge();
    v.as_object_mut().unwrap().remove("tallyOverrideReason");
    assert!(verdict::validate_judge_verdict(&v, &nicks(), &tally(0, 0)).is_err());
    let mut result = verdict::validate_judge_verdict(&judge(), &nicks(), &tally(0, 0)).unwrap();
    result.weighted_totals[0].total = 100.0;
    assert!(verdict::validate_committed_verdict(&result, &nicks(), &tally(0, 0)).is_err());
}
#[test]
fn round_schema_exact_members_integer_bounds_and_character_limits() {
    let good =
        json!({"scores":{"a":{"score":5,"reason":"Good."},"b":{"score":5,"reason":"Good."}}});
    assert!(verdict::validate_round_response(&good, &nicks()).is_ok());
    for value in [json!(0), json!(11), json!(1.1), json!("5"), json!(-1)] {
        let mut v = good.clone();
        v["scores"]["a"]["score"] = value;
        assert!(verdict::validate_round_response(&v, &nicks()).is_err());
    }
    let mut v = good.clone();
    v["scores"]["c"] = v["scores"]["a"].clone();
    assert!(verdict::validate_round_response(&v, &nicks()).is_err());
    let mut v = good.clone();
    v["scores"].as_object_mut().unwrap().remove("b");
    assert!(verdict::validate_round_response(&v, &nicks()).is_err());
    let mut v = good;
    v["scores"]["a"]["reason"] = json!("é".repeat(200));
    assert!(verdict::validate_round_response(&v, &nicks()).is_ok());
    v["scores"]["a"]["reason"] = json!("é".repeat(201));
    assert!(verdict::validate_round_response(&v, &nicks()).is_err());
    v["tally"] = json!({});
    assert!(verdict::validate_round_response(&v, &nicks()).is_err());
}
