use serde_json::json;
use squad_protocol::{rank::*, *};
#[test]
fn batch_requires_exact_unique_pairs_and_bounded_reasons() {
    let assigned = vec!["P1".into(), "P2".into()];
    let valid = json!({"choices":[{"pairId":"P1","winner":"A","reason":"Higher impact"},{"pairId":"P2","winner":"B","reason":"Lower effort"}]});
    let batch: RankBatch = validation::decode(&valid).unwrap();
    batch.validate(&assigned).unwrap();
    let mut missing = batch.clone();
    missing.choices.pop();
    assert!(missing.validate(&assigned).is_err());
    let mut duplicate = batch.clone();
    duplicate.choices[1].pair_id = "P1".into();
    assert!(duplicate.validate(&assigned).is_err());
    let mut outside = valid.clone();
    outside["choices"][0]["winner"] = json!("C");
    assert!(validation::decode::<RankBatch>(&outside).is_err());
    let mut verbose = batch;
    verbose.choices[0].reason = "word ".repeat(13);
    assert!(verbose.validate(&assigned).is_err());
}
#[test]
fn explanation_cannot_reorder_or_omit_cycles() {
    let valid = json!({"top":[{"itemId":"I1","why":"Impact"},{"itemId":"I2","why":"Effort"},{"itemId":"I3","why":"Risk"}],"cycles":[{"cycleId":"C1","reading":"Criterion conflict"}],"caveats":"Uncertain"});
    let e: RankExplanation = validation::decode(&valid).unwrap();
    e.validate(
        &["I1".into(), "I2".into(), "I3".into()],
        &["C1".into()],
        500,
    )
    .unwrap();
    let mut missing = e;
    missing.cycles.clear();
    assert!(
        missing
            .validate(
                &["I1".into(), "I2".into(), "I3".into()],
                &["C1".into()],
                500
            )
            .is_err()
    );
    for key in ["order", "position", "score"] {
        let mut bad = valid.clone();
        bad[key] = json!(["I3", "I2", "I1"]);
        assert!(validation::decode::<RankExplanation>(&bad).is_err());
    }
}
#[test]
fn schema_is_strict_and_registered() {
    for kind in [WorkKind::RankCompare, WorkKind::RankExplain] {
        let s = output_schema(&kind).unwrap();
        assert_eq!(s["additionalProperties"], false);
        assert!(s["required"].is_array());
    }
    assert!(validate_artifact("rank.unknown", &json!({})).is_err());
}
