use serde_json::json;
use squad_core::orchestrator::debugging::*;
use squad_protocol::debugging::*;
fn rows() -> Vec<DebuggingLiveHypothesis> {
    ["H1", "H2"]
        .iter()
        .map(|id| DebuggingLiveHypothesis {
            hypothesis: DebuggingMergedHypothesis {
                hypothesis_id: id.to_string(),
                cause: id.to_string(),
                sources: vec!["T:1".into()],
                supporting: vec![],
                contradicting: vec![],
                prior_spread: vec![DebuggingPrior::High],
            },
            status: DebuggingHypothesisStatus::Live,
            consistent_results: 0,
            failed_prediction: None,
        })
        .collect()
}
fn selection(a: &str, b: &str) -> DebuggingSelection {
    serde_json::from_value(json!({"testRef":"P:t","instruction":"Read the output","outcomes":[{"label":"O1","description":"A"},{"label":"O2","description":"B"}],"predictions":[{"hypothesisId":"H1","outcome":a},{"hypothesisId":"H2","outcome":b}],"addedHypotheses":[],"notSelected":[]})).unwrap()
}
#[test]
fn hand_computed_elimination_and_patch_gate() {
    let r = rows();
    assert!(!patch_allowed(&r));
    for (a, b, outcome, expected) in [
        ("O1", "O2", "O1", (1, 1)),
        ("O1", "O2", "O2", (1, 1)),
        ("O1", "O2", "other", (2, 0)),
        ("O1", "O2", "unknown", (2, 0)),
        ("disputed", "cannot_predict", "O1", (2, 0)),
        ("O2", "O2", "O1", (0, 0)),
    ] {
        let result = DebuggingResult {
            round: 1,
            outcome: outcome.into(),
            evidence: String::new(),
            unexpected: outcome == "other",
        };
        let e = eliminate(&r, &selection(a, b), &result);
        assert_eq!(
            (
                e.hypotheses
                    .iter()
                    .filter(|h| h.status == DebuggingHypothesisStatus::Live)
                    .count(),
                e.hypotheses
                    .iter()
                    .map(|h| h.consistent_results)
                    .sum::<u32>()
            ),
            expected
        );
        assert_eq!(patch_allowed(&e.hypotheses), expected == (1, 1));
    }
    let mut r = rows();
    r[1].status = DebuggingHypothesisStatus::Eliminated;
    assert!(!patch_allowed(&r));
    r[0].consistent_results = 1;
    assert!(patch_allowed(&r));
    assert_eq!(loop_end(&r, 1, 3), Some("isolated"));
    r[0].status = DebuggingHypothesisStatus::Eliminated;
    assert_eq!(loop_end(&r, 1, 3), Some("all_eliminated"));
    assert_eq!(loop_end(&rows(), 3, 3), Some("rounds_complete"));
    assert_eq!(loop_end(&rows(), 1, 3), None);
}
#[test]
fn call_bounds_and_pasted_evidence_cap() {
    assert_eq!(call_bounds(6, 3), (29, 63));
    assert_eq!(call_bounds(12, 6), (92, 192));
    let raw = "word ".repeat(600);
    assert_eq!(prompt_evidence(&raw).split_whitespace().count(), 400);
    assert_eq!(raw.split_whitespace().count(), 600);
}
#[test]
fn blind_prose_preserves_identifiers() {
    let mut v = json!({"cause":"Member0 uses fixture/model", "localId":"one"});
    blind_value(&mut v, &["Member0".into(), "fixture/model".into()]);
    assert!(!v.to_string().contains("Member0"));
    assert_eq!(v["localId"], "one");
}
