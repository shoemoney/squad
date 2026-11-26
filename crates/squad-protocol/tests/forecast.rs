use serde_json::json;
use squad_protocol::{forecast::*, *};
fn estimate() -> serde_json::Value {
    json!({"probabilityPct":50,"baseRate":{"referenceClass":"Comparable projects","ratePct":null},"factors":[{"direction":"up","text":"Funding"},{"direction":"down","text":"Schedule"},{"direction":"up","text":"Prototype"}],"rationale":"Mixed evidence"})
}
#[test]
fn strict_estimate_endpoints_factors_and_nullable() {
    let good = estimate();
    validate_output(&WorkKind::ForecastEstimate, &json!({}), &good).unwrap();
    for p in [0, 100] {
        let mut bad = good.clone();
        bad["probabilityPct"] = json!(p);
        assert!(validate_output(&WorkKind::ForecastEstimate, &json!({}), &bad).is_err());
    }
    for n in [2, 4] {
        let mut bad = good.clone();
        bad["factors"] = json!(vec![good["factors"][0].clone(); n]);
        assert!(validate_output(&WorkKind::ForecastEstimate, &json!({}), &bad).is_err());
    }
    let mut missing = good;
    missing["baseRate"]
        .as_object_mut()
        .unwrap()
        .remove("ratePct");
    assert!(validate_output(&WorkKind::ForecastEstimate, &json!({}), &missing).is_err());
}
#[test]
fn labels_schema_and_judge_probability_are_enforced() {
    let data = json!({"labels":["F1","F2"]});
    let good = json!({"probabilityPct":40,"changeReason":"Evidence","citedLabels":["F1"]});
    validate_output(&WorkKind::ForecastRevise, &data, &good).unwrap();
    let mut bad = good;
    bad["citedLabels"] = json!(["F3"]);
    assert!(validate_output(&WorkKind::ForecastRevise, &data, &bad).is_err());
    let mut cruxes = json!({"cruxes":[],"signposts":[],"sharedAssumptions":[],"weakEvidence":[]});
    validate_output(&WorkKind::ForecastCruxes, &data, &cruxes).unwrap();
    cruxes["probabilityPct"] = json!(50);
    assert!(validate_output(&WorkKind::ForecastCruxes, &data, &cruxes).is_err());
}
#[test]
fn calendar_dates_are_real_and_never_panic_on_unicode() {
    let mut frame = ForecastFrame {
        question: Text::new("Will it launch?").unwrap(),
        resolution_criteria: Text::new("Public launch").unwrap(),
        resolution_date: "2028-02-29".into(),
        resolution_source: Text::new("Official site").unwrap(),
        void_rule: Text::new("Project canceled").unwrap(),
        forecastable: true,
        reason: Text::new("Observable").unwrap(),
    };
    frame.validate().unwrap();
    for date in ["2027-02-29", "2027-11-31", "💥💥xx", "2027-00-01"] {
        frame.resolution_date = date.into();
        assert!(frame.validate().is_err());
    }
}
#[test]
fn parser_example_and_registration_are_enabled() {
    assert_eq!(ContestKind::Forecast.is_built(), ENABLED);
    let ctx = parser::ParseContext {
        roster: ["a", "b", "c"].map(|s| s.parse().unwrap()).to_vec(),
        ..Default::default()
    };
    let parsed = parse(
        "@all $forecast Will X ship by 2027-12-31?",
        &parser::SubmissionMeta::default(),
        &ctx,
    );
    if ENABLED {
        assert!(matches!(parsed.unwrap(), ParsedInput::Forecast { .. }));
    } else {
        assert_eq!(parsed.unwrap_err().code, ErrorCode::COMMAND_NOT_IMPLEMENTED);
    }
}
