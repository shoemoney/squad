use serde_json::json;
use squad_core::{export::debugging::*, orchestrator::debugging::validate_output};
use squad_protocol::{debugging::*, *};
#[test]
fn unsafe_unproposed_nonsplitting_and_changed_procedure_rejected() {
    let rows = json!([{"hypothesis":{"hypothesisId":"H1","cause":"A","sources":["A:a"],"supporting":[],"contradicting":[],"priorSpread":["high"]},"status":"live","consistentResults":0,"failedPrediction":null},{"hypothesis":{"hypothesisId":"H2","cause":"B","sources":["A:b"],"supporting":[],"contradicting":[],"priorSpread":["low"]},"status":"live","consistentResults":0,"failedPrediction":null}]);
    let mut data = json!({"differential":rows,"allowNewHypothesis":false,"proposals":{"P":{"tests":[{"localId":"t","test":"Read state","procedure":"Read state","effort":"minutes","safety":"read_only","outcomes":["A","B"],"predictions":[{"hypothesisId":"H1","outcome":"A","cannotPredictReason":null},{"hypothesisId":"H2","outcome":"B","cannotPredictReason":null}]}],"newHypothesis":null}}});
    let s = json!({"testRef":"P:t","instruction":"Read state","outcomes":[{"label":"O1","description":"A"},{"label":"O2","description":"B"}],"predictions":[{"hypothesisId":"H1","outcome":"O1"},{"hypothesisId":"H2","outcome":"O2"}],"addedHypotheses":[],"notSelected":[]});
    validate_output(&WorkKind::DebuggingSelect, &data, &s).unwrap();
    for (key, v) in [
        ("testRef", json!("invented")),
        ("instruction", json!("Do something else")),
        (
            "predictions",
            json!([{"hypothesisId":"H1","outcome":"O1"},{"hypothesisId":"H2","outcome":"O1"}]),
        ),
    ] {
        let mut invalid = s.clone();
        invalid[key] = v;
        assert!(validate_output(&WorkKind::DebuggingSelect, &data, &invalid).is_err());
    }
    data["proposals"]["P"]["tests"][0]["safety"] = json!("risky");
    assert!(validate_output(&WorkKind::DebuggingSelect, &data, &s).is_err());
    let patch = json!({"summary":"A cause","nextTest":null,"patch":"Change DNS","patchRationale":"A cause","risks":[]});
    data["patchAllowed"] = json!(false);
    assert!(validate_output(&WorkKind::DebuggingReport, &data, &patch).is_err());
    data["patchAllowed"] = json!(true);
    assert!(validate_output(&WorkKind::DebuggingReport, &data, &patch).is_err());
    data["differential"][1]["status"] = json!("eliminated");
    assert!(validate_output(&WorkKind::DebuggingReport, &data, &patch).is_err());
    data["differential"][0]["consistentResults"] = json!(1);
    validate_output(&WorkKind::DebuggingReport, &data, &patch).unwrap();
}
#[test]
fn report_retains_raw_results_and_unanchored_quotes() {
    let r = DebuggingReport {
        brief: "A failure".into(),
        limits: "Read only".into(),
        differential: vec![],
        tables: Default::default(),
        provenance: Default::default(),
        rounds: vec![DebuggingRound {
            round: 1,
            proposals: Default::default(),
            selection: None,
            result: Some(DebuggingResult {
                round: 1,
                outcome: "other".into(),
                evidence: "RAW OBSERVATION".into(),
                unexpected: true,
            }),
            elimination: None,
        }],
        quotes: vec![DebuggingQuoteCheck {
            quote: "Unsupported quote".into(),
            unanchored: true,
        }],
        skipped: vec!["member1 · hypothesizing".into()],
        raw_answers: vec!["other RAW OBSERVATION".into()],
        reason: "rounds_complete".into(),
        patch_allowed: false,
        narrative: None,
    };
    let out = report_text(&r, true);
    for text in [
        "DIFFERENTIAL",
        "RAW OBSERVATION",
        "unexpected",
        "Unanchored quote (not evidence)",
        "Skipped:",
        "No patch: the evidence does not yet isolate a cause.",
    ] {
        assert!(out.contains(text), "{text}");
    }
    assert!(!out.contains("confirmed"));
}
#[test]
fn merge_preserves_minorities_quotes_and_prior_disagreement() {
    let data = json!({"tables":{"Table 1":{"hypotheses":[{"localId":"a","cause":"DNS","supporting":["Network requests fail"],"contradicting":[],"prior":"high"}]},"Table 2":{"hypotheses":[{"localId":"b","cause":"DNS","supporting":[],"contradicting":["Health checks pass"],"prior":"low"}]}}});
    let merged = json!({"hypotheses":[{"hypothesisId":"H1","cause":"DNS","sources":["Table 1:a","Table 2:b"],"supporting":["Network requests fail"],"contradicting":["Health checks pass"],"priorSpread":["high","low"]}]});
    validate_output(&WorkKind::DebuggingMerge, &data, &merged).unwrap();
    for (field, value) in [
        ("sources", json!(["Table 1:a"])),
        ("contradicting", json!([])),
        ("priorSpread", json!(["high"])),
    ] {
        let mut invalid = merged.clone();
        invalid["hypotheses"][0][field] = value;
        assert!(validate_output(&WorkKind::DebuggingMerge, &data, &invalid).is_err());
    }
}
